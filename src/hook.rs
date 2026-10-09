//! Claude Code hooks, and the inbox they share with the viewer.

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use anyhow::Result;
use serde_json::Value;

/// Images pasted into the viewer for a session wait here until the next prompt picks them up.
pub fn inbox_dir(session: &str) -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".megane").join("inbox").join(session)
}

/// UserPromptSubmit hook: tells Claude about images pasted since the last prompt.
/// Whatever this prints on stdout is added to the prompt's context.
pub fn prompt() -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let v: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
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
