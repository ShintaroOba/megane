//! Reading Claude Code transcripts (~/.claude/projects/<project>/<session>.jsonl).

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use serde_json::{json, Value};

const MAX_TOOL_TEXT: usize = 4000;
const MAX_SUMMARY: usize = 200;

#[derive(Serialize, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub project: String,
    pub title: Option<String>,
    pub modified: u64,
    #[serde(skip)]
    pub path: PathBuf,
}

pub fn projects_dir() -> PathBuf {
    let base = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".claude"));
    base.join("projects")
}

/// Claude Code names a project directory after its cwd with every non-alphanumeric char replaced by '-'.
fn encode_cwd(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn mtime(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

fn sessions_in(dir: &Path) -> Vec<SessionInfo> {
    let project = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let Ok(entries) = fs::read_dir(dir) else { return vec![] };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .map(|path| SessionInfo {
            id: path.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
            project: project.clone(),
            title: None,
            modified: mtime(&path),
            path,
        })
        .collect()
}

pub fn list_sessions(limit: usize) -> Vec<SessionInfo> {
    let Ok(projects) = fs::read_dir(projects_dir()) else { return vec![] };
    let mut all: Vec<SessionInfo> = projects
        .flatten()
        .filter(|e| e.path().is_dir())
        .flat_map(|e| sessions_in(&e.path()))
        .collect();
    all.sort_by(|a, b| b.modified.cmp(&a.modified));
    all.truncate(limit);
    for s in &mut all {
        s.title = read_title(&s.path);
    }
    all
}

pub fn find_session(id: &str) -> Option<SessionInfo> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let projects = fs::read_dir(projects_dir()).ok()?;
    projects.flatten().find_map(|e| {
        let path = e.path().join(format!("{id}.jsonl"));
        path.is_file().then(|| SessionInfo {
            id: id.to_string(),
            project: e.file_name().to_string_lossy().into_owned(),
            title: read_title(&path),
            modified: mtime(&path),
            path,
        })
    })
}

pub fn latest_for_cwd(cwd: &Path) -> Option<SessionInfo> {
    let mut sessions = sessions_in(&projects_dir().join(encode_cwd(cwd)));
    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
    sessions.into_iter().next()
}

/// The newest `ai-title` (or last prompt) record, looked up in the file's tail to keep listing cheap.
fn read_title(path: &Path) -> Option<String> {
    const TAIL: u64 = 256 * 1024;
    let mut f = fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(TAIL))).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    let mut prompt = None;
    for line in text.lines().rev() {
        if !line.contains("\"ai-title\"") && !line.contains("\"last-prompt\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(t) = v.get("aiTitle").and_then(Value::as_str) {
            return Some(t.to_string());
        }
        if prompt.is_none() {
            prompt = v.get("lastPrompt").and_then(Value::as_str).map(|s| truncate(s, 80));
        }
    }
    prompt
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

fn strip_tag(text: &str, tag: &str) -> String {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        out.push_str(&rest[..start]);
        match rest[start..].find(&close) {
            Some(end) => rest = &rest[start + end + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn clean_user_text(text: &str) -> String {
    let mut t = text.to_string();
    for tag in ["system-reminder", "command-message", "local-command-stdout", "local-command-caveat"] {
        t = strip_tag(&t, tag);
    }
    t.trim().to_string()
}

fn image_block(v: &Value) -> Option<Value> {
    let src = v.get("source")?;
    Some(json!({
        "type": "image",
        "media_type": src.get("media_type").and_then(Value::as_str).unwrap_or("image/png"),
        "data": src.get("data")?.as_str()?,
    }))
}

fn tool_summary(input: &Value) -> String {
    const KEYS: [&str; 8] =
        ["command", "file_path", "path", "pattern", "url", "query", "description", "prompt"];
    KEYS.iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .map(|s| truncate(s.lines().next().unwrap_or(""), MAX_SUMMARY))
        .unwrap_or_default()
}

fn tool_result_block(v: &Value) -> Value {
    let mut text = String::new();
    let mut images = Vec::new();
    match v.get("content") {
        Some(Value::String(s)) => text.push_str(s),
        Some(Value::Array(items)) => {
            for item in items {
                match item.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        text.push_str(item.get("text").and_then(Value::as_str).unwrap_or(""));
                        text.push('\n');
                    }
                    Some("image") => images.extend(image_block(item)),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    json!({
        "type": "tool_result",
        "text": truncate(text.trim_end(), MAX_TOOL_TEXT),
        "images": images,
        "is_error": v.get("is_error").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Turns one transcript line into a render-ready message, or None for records the viewer ignores.
pub fn normalize(line: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(line).ok()?;
    let kind = v.get("type")?.as_str()?;
    if !matches!(kind, "user" | "assistant")
        || v.get("isSidechain").and_then(Value::as_bool) == Some(true)
        || v.get("isMeta").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let content = v.get("message")?.get("content")?;
    let mut blocks = Vec::new();
    match content {
        Value::String(s) => {
            let t = clean_user_text(s);
            if !t.is_empty() {
                blocks.push(json!({ "type": "text", "text": t }));
            }
        }
        Value::Array(items) => {
            for item in items {
                match item.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let raw = item.get("text").and_then(Value::as_str).unwrap_or("");
                        let t = if kind == "user" { clean_user_text(raw) } else { raw.to_string() };
                        if !t.is_empty() {
                            blocks.push(json!({ "type": "text", "text": t }));
                        }
                    }
                    Some("image") => blocks.extend(image_block(item)),
                    Some("tool_use") => blocks.push(json!({
                        "type": "tool_use",
                        "name": item.get("name").and_then(Value::as_str).unwrap_or("tool"),
                        "summary": item.get("input").map(tool_summary).unwrap_or_default(),
                    })),
                    Some("tool_result") => blocks.push(tool_result_block(item)),
                    _ => {}
                }
            }
        }
        _ => return None,
    }
    if blocks.is_empty() {
        return None;
    }
    Some(json!({
        "uuid": v.get("uuid"),
        "role": kind,
        "timestamp": v.get("timestamp"),
        "blocks": blocks,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_cwd_like_claude_code() {
        assert_eq!(encode_cwd(Path::new(r"C:\Users\me\ghq\github.com\a.b")), "C--Users-me-ghq-github-com-a-b");
        assert_eq!(encode_cwd(Path::new("/home/me/src")), "-home-me-src");
    }

    #[test]
    fn strips_reminders_from_user_text() {
        let line = r#"{"type":"user","uuid":"u","message":{"role":"user","content":"draw it<system-reminder>secret</system-reminder>"}}"#;
        let msg = normalize(line).unwrap();
        assert_eq!(msg["role"], "user");
        assert_eq!(msg["blocks"][0]["text"], "draw it");
    }

    #[test]
    fn skips_sidechain_meta_and_other_records() {
        assert!(normalize(r#"{"type":"user","isSidechain":true,"message":{"content":"x"}}"#).is_none());
        assert!(normalize(r#"{"type":"user","isMeta":true,"message":{"content":"x"}}"#).is_none());
        assert!(normalize(r#"{"type":"ai-title","aiTitle":"t"}"#).is_none());
        assert!(normalize("not json").is_none());
    }

    #[test]
    fn summarises_tool_calls_and_results() {
        let call = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hm"},{"type":"tool_use","name":"Bash","input":{"command":"ls -la\npwd"}}]}}"#;
        let msg = normalize(call).unwrap();
        assert_eq!(msg["blocks"].as_array().unwrap().len(), 1);
        assert_eq!(msg["blocks"][0]["summary"], "ls -la");

        let result = r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":[{"type":"text","text":"boom"},{"type":"image","source":{"media_type":"image/png","data":"AA=="}}]}]}}"#;
        let msg = normalize(result).unwrap();
        let block = &msg["blocks"][0];
        assert_eq!(block["text"], "boom");
        assert_eq!(block["is_error"], true);
        assert_eq!(block["images"][0]["data"], "AA==");
    }

    #[test]
    fn truncates_by_chars() {
        assert_eq!(truncate("眼鏡眼鏡", 2), "眼鏡…");
        assert_eq!(truncate("ab", 2), "ab");
    }
}
