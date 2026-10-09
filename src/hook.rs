//! Claude Code hooks, and the inbox they share with the viewer.

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use serde_json::{json, Value};

use crate::server::HOOK_HEADER;

/// Images pasted into the viewer for a session wait here until the next prompt picks them up.
pub fn inbox_dir(session: &str) -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".megane").join("inbox").join(session)
}

fn read_input() -> Result<Value> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    Ok(serde_json::from_str(&input).unwrap_or(Value::Null))
}

/// UserPromptSubmit hook: tells Claude about images pasted since the last prompt.
/// Whatever this prints on stdout is added to the prompt's context.
pub fn prompt() -> Result<()> {
    let v = read_input()?;
    let Some(session) = v.get("session_id").and_then(Value::as_str) else { return Ok(()) };

    let inbox = inbox_dir(session);
    let Ok(entries) = fs::read_dir(&inbox) else { return Ok(()) };
    let mut pending: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    if pending.is_empty() {
        return Ok(());
    }
    pending.sort();

    let sent = inbox.join("sent");
    fs::create_dir_all(&sent)?;
    println!("The user pasted these images in the MEGANE viewer. Read them with the Read tool:");
    for path in pending {
        let target = sent.join(path.file_name().unwrap_or_default());
        fs::rename(&path, &target)?;
        println!("- {}", target.display());
    }
    Ok(())
}

/// Which hook event is asking.
#[derive(Clone, Copy)]
pub enum Ask {
    /// PreToolUse on AskUserQuestion / ExitPlanMode.
    PreTool,
    /// PermissionRequest on any other tool.
    Permission,
}

/// Hands a question, plan approval or permission prompt to the viewer and prints the viewer's
/// decision in the hook output format. Prints nothing (so Claude Code carries on in the
/// terminal as usual) when no viewer is ready to answer, the server is not running, or the
/// user chose to answer in the terminal.
pub fn ask(port: u16, kind: Ask) -> Result<()> {
    let input = read_input()?;
    let (Some(session), Some(tool)) = (
        input.get("session_id").and_then(Value::as_str),
        input.get("tool_name").and_then(Value::as_str),
    ) else {
        return Ok(());
    };
    let tool_input = input.get("tool_input").cloned().unwrap_or(json!({}));
    let question = matches!(tool, "AskUserQuestion" | "ExitPlanMode");
    match kind {
        // Only these two tools are answered at PreToolUse; everything else waits for the
        // permission prompt, and these two never go through the permission hook.
        Ask::PreTool if !question => return Ok(()),
        Ask::Permission if question => return Ok(()),
        _ => {}
    }

    let request = json!({
        "kind": match (kind, tool) {
            (Ask::PreTool, "AskUserQuestion") => "question",
            (Ask::PreTool, _) => "plan",
            (Ask::Permission, _) => "permission",
        },
        "tool_name": tool,
        "tool_input": tool_input,
        "suggestions": input.get("permission_suggestions").cloned().unwrap_or(Value::Null),
    });
    let Some(answer) = post(port, &format!("/api/hook/{session}"), &request.to_string()) else {
        return Ok(());
    };
    if let Some(output) = hook_output(kind, &input, &answer) {
        println!("{output}");
    }
    Ok(())
}

/// Turns the viewer's answer into Claude Code's hook output.
fn hook_output(kind: Ask, input: &Value, answer: &Value) -> Option<Value> {
    let tool_input = input.get("tool_input").cloned().unwrap_or(json!({}));
    let message = answer.get("message").and_then(Value::as_str).unwrap_or("").trim();
    let action = answer.get("action").and_then(Value::as_str)?;
    match kind {
        Ask::PreTool => {
            let (decision, reason, updated) = match action {
                // AskUserQuestion: the original questions plus {"<question>": "<label>"}.
                "answer" => {
                    let mut updated = tool_input;
                    updated["answers"] = answer.get("answers").cloned().unwrap_or(json!({}));
                    ("allow", "Answered in the MEGANE viewer".to_string(), Some(updated))
                }
                // ExitPlanMode: approving passes the plan through unchanged.
                "approve" => ("allow", "Plan approved in the MEGANE viewer".to_string(), Some(tool_input)),
                "reject" if message.is_empty() => {
                    ("deny", "The user rejected the plan in the MEGANE viewer.".to_string(), None)
                }
                "reject" => {
                    ("deny", format!("The user rejected the plan in the MEGANE viewer: {message}"), None)
                }
                "dismiss" => ("deny", "The user dismissed the question in the MEGANE viewer.".to_string(), None),
                _ => return None,
            };
            let mut out = json!({
                "hookEventName": "PreToolUse",
                "permissionDecision": decision,
                "permissionDecisionReason": reason,
            });
            if let Some(updated) = updated {
                out["updatedInput"] = updated;
            }
            Some(json!({ "hookSpecificOutput": out }))
        }
        Ask::Permission => {
            let decision = match action {
                "allow" => json!({ "behavior": "allow" }),
                "always" => match input.get("permission_suggestions") {
                    Some(s) if s.as_array().is_some_and(|a| !a.is_empty()) => {
                        json!({ "behavior": "allow", "updatedPermissions": s })
                    }
                    _ => json!({ "behavior": "allow" }),
                },
                "deny" => json!({
                    "behavior": "deny",
                    "message": if message.is_empty() {
                        "The user denied this in the MEGANE viewer.".to_string()
                    } else {
                        format!("The user denied this in the MEGANE viewer: {message}")
                    },
                }),
                _ => return None,
            };
            Some(json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } }))
        }
    }
}

/// A minimal HTTP/1.1 POST to the local server. Returns the JSON body of a 200 response;
/// None for anything else (no server, 204 fall-through, errors).
fn post(port: u16, path: &str, body: &str) -> Option<Value> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(600))).ok()?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\n\
         {HOOK_HEADER}: 1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    let response = String::from_utf8_lossy(&response);
    let (head, body) = response.split_once("\r\n\r\n")?;
    if head.split(' ').nth(1) != Some("200") {
        return None;
    }
    serde_json::from_str(body).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_a_question_with_the_original_input() {
        let input = json!({ "tool_input": { "questions": [{ "question": "Q?" }] } });
        let out = hook_output(Ask::PreTool, &input, &json!({ "action": "answer", "answers": { "Q?": "Yes" } })).unwrap();
        let o = &out["hookSpecificOutput"];
        assert_eq!(o["permissionDecision"], "allow");
        assert_eq!(o["updatedInput"]["questions"][0]["question"], "Q?");
        assert_eq!(o["updatedInput"]["answers"]["Q?"], "Yes");
    }

    #[test]
    fn rejects_a_plan_with_feedback() {
        let input = json!({ "tool_input": { "plan": "p" } });
        let out = hook_output(Ask::PreTool, &input, &json!({ "action": "reject", "message": "split it" })).unwrap();
        assert_eq!(out["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(out["hookSpecificOutput"]["permissionDecisionReason"].as_str().unwrap().ends_with("split it"));
    }

    #[test]
    fn always_allow_applies_suggestions() {
        let input = json!({ "permission_suggestions": [{ "type": "addRules" }] });
        let out = hook_output(Ask::Permission, &input, &json!({ "action": "always" })).unwrap();
        let d = &out["hookSpecificOutput"]["decision"];
        assert_eq!(d["behavior"], "allow");
        assert_eq!(d["updatedPermissions"][0]["type"], "addRules");
        assert!(hook_output(Ask::Permission, &input, &json!({ "action": "terminal" })).is_none());
    }
}
