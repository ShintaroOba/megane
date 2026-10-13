//! Typing into the terminal a session runs in, through whatever that terminal offers.
//!
//! MEGANE never owns the `claude` process. When the terminal around it can accept input from
//! outside (Orca, tmux, WezTerm), the viewer's composer sends keystrokes there, exactly as if
//! they were typed. Anywhere else the viewer stays read-only. Which terminal a session lives in
//! is detected from the environment Claude Code passes to hooks, and recorded per session.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Bracketed paste markers: multi-line text arrives as one paste instead of one Enter per line.
const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Terminal {
    Orca { cli: String, tab_id: String, pane_key: Option<String> },
    Tmux { pane: String, socket: Option<String> },
    Wezterm { cli: String, pane: String },
}

impl Terminal {
    pub fn kind(&self) -> &'static str {
        match self {
            Terminal::Orca { .. } => "orca",
            Terminal::Tmux { .. } => "tmux",
            Terminal::Wezterm { .. } => "wezterm",
        }
    }
}

pub enum Input<'a> {
    /// Text, optionally followed by Enter.
    Text { text: &'a str, submit: bool },
    /// The Esc key: interrupts Claude while it works.
    Escape,
}

/// Looks for a known terminal in an environment (normally our own, inherited from Claude Code).
pub fn detect(env: impl Fn(&str) -> Option<String>) -> Option<Terminal> {
    let set = |k: &str| env(k).filter(|v| !v.is_empty());
    if let Some(tab_id) = set("ORCA_TAB_ID") {
        if let Some(cli) = find_orca_cli(&env) {
            return Some(Terminal::Orca { cli, tab_id, pane_key: set("ORCA_PANE_KEY") });
        }
    }
    if let Some(pane) = set("TMUX_PANE") {
        // $TMUX is "<socket>,<pid>,<session>".
        let socket = set("TMUX").and_then(|t| t.split(',').next().map(str::to_string));
        return Some(Terminal::Tmux { pane, socket });
    }
    if let Some(pane) = set("WEZTERM_PANE") {
        let cli = find_on_path(&env, "wezterm")
            .or_else(|| set("WEZTERM_EXECUTABLE_DIR").map(|d| exe_in(Path::new(&d), "wezterm")))?;
        return Some(Terminal::Wezterm { cli, pane });
    }
    None
}

fn exe_in(dir: &Path, name: &str) -> String {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    dir.join(file).to_string_lossy().into_owned()
}

fn find_on_path(env: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    let path = env("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| exe_in(&dir, name))
        .find(|candidate| Path::new(candidate).is_file())
}

fn find_orca_cli(env: &impl Fn(&str) -> Option<String>) -> Option<String> {
    find_on_path(env, "orca").or_else(|| {
        // Windows installs the CLI next to the app without always putting it on PATH.
        let local = env("LOCALAPPDATA")?;
        let cli = Path::new(&local).join(r"Programs\orca\resources\bin\orca.exe");
        cli.is_file().then(|| cli.to_string_lossy().into_owned())
    })
}

fn record_path(session: &str) -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".megane").join("terminals").join(format!("{session}.json"))
}

fn valid_id(session: &str) -> bool {
    !session.is_empty() && session.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Remembers which terminal `session` runs in, if it is one we can type into.
pub fn record(session: &str) {
    if !valid_id(session) {
        return;
    }
    let Some(terminal) = detect(|k| std::env::var(k).ok()) else { return };
    let path = record_path(session);
    if load(session).as_ref() == Some(&terminal) {
        return;
    }
    if let (Some(dir), Ok(json)) = (path.parent(), serde_json::to_vec(&terminal)) {
        let _ = fs::create_dir_all(dir).and_then(|()| fs::write(&path, json));
    }
}

pub fn load(session: &str) -> Option<Terminal> {
    if !valid_id(session) {
        return None;
    }
    serde_json::from_slice(&fs::read(record_path(session)).ok()?).ok()
}

/// Opens `url` as a tab of the terminal app when it has a built-in browser (Orca).
/// Returns false when the caller should use the system browser instead.
pub fn open_url(terminal: &Terminal, url: &str) -> bool {
    match terminal {
        Terminal::Orca { cli, .. } => run(cli, &["tab", "create", "--url", url]).is_ok(),
        _ => false,
    }
}

pub fn send(terminal: &Terminal, input: Input) -> Result<()> {
    let wrap = |text: &str| -> String {
        if text.contains('\n') {
            format!("{PASTE_START}{text}{PASTE_END}")
        } else {
            text.to_string()
        }
    };
    match (terminal, input) {
        (Terminal::Orca { cli, tab_id, pane_key }, input) => {
            let handle = orca_handle(cli, tab_id, pane_key.as_deref())?;
            match input {
                Input::Text { text, submit } => {
                    let text = wrap(text);
                    let mut args = vec!["terminal", "send", "--terminal", &handle, "--text", &text];
                    if submit {
                        args.push("--enter");
                    }
                    run(cli, &args)
                }
                Input::Escape => run(cli, &["terminal", "send", "--terminal", &handle, "--text", "\x1b"]),
            }
        }
        (Terminal::Tmux { pane, socket }, input) => {
            let mut base: Vec<&str> = Vec::new();
            if let Some(socket) = socket {
                base.extend(["-S", socket]);
            }
            let keys = |extra: &[&str]| {
                let mut args = base.clone();
                args.extend(["send-keys", "-t", pane.as_str()]);
                args.extend(extra);
                run("tmux", &args)
            };
            match input {
                Input::Text { text, submit } => {
                    keys(&["-l", &wrap(text)])?;
                    if submit {
                        keys(&["Enter"])?;
                    }
                    Ok(())
                }
                Input::Escape => keys(&["Escape"]),
            }
        }
        (Terminal::Wezterm { cli, pane }, input) => {
            let send = |text: &str| run(cli, &["cli", "send-text", "--pane-id", pane, "--no-paste", text]);
            match input {
                Input::Text { text, submit } => {
                    send(&wrap(text))?;
                    if submit {
                        send("\r")?;
                    }
                    Ok(())
                }
                Input::Escape => send("\x1b"),
            }
        }
    }
}

/// Orca addresses terminals by a runtime handle; find the one in our tab and pane.
fn orca_handle(cli: &str, tab_id: &str, pane_key: Option<&str>) -> Result<String> {
    let out = output(cli, &["terminal", "list", "--json"])?;
    let v: serde_json::Value = serde_json::from_str(&out).context("unexpected `orca terminal list` output")?;
    let in_tab: Vec<&serde_json::Value> = v["result"]["terminals"]
        .as_array()
        .map(|a| a.iter().filter(|t| t["tabId"] == tab_id && t["writable"] != false).collect())
        .unwrap_or_default();
    let pick = in_tab
        .iter()
        .find(|t| {
            pane_key.is_some_and(|key| t["leafId"].as_str().is_some_and(|leaf| key.contains(leaf)))
        })
        .or_else(|| (in_tab.len() == 1).then(|| &in_tab[0]))
        .or_else(|| in_tab.iter().find(|t| t["agentIdentity"] == "claude"));
    match pick.and_then(|t| t["handle"].as_str()) {
        Some(handle) => Ok(handle.to_string()),
        None => bail!("the Orca terminal for this session is not open"),
    }
}

fn command(program: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        // The server runs without a console; keep child consoles from flashing up.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn run(program: &str, args: &[&str]) -> Result<()> {
    output(program, args).map(|_| ())
}

fn output(program: &str, args: &[&str]) -> Result<String> {
    let out = command(program, args).output().with_context(|| format!("could not run {program}"))?;
    if !out.status.success() {
        bail!("{program} failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn detects_tmux_with_its_socket() {
        let t = detect(env(&[("TMUX", "/tmp/tmux-1000/default,42,0"), ("TMUX_PANE", "%3")])).unwrap();
        assert_eq!(t, Terminal::Tmux { pane: "%3".into(), socket: Some("/tmp/tmux-1000/default".into()) });
    }

    #[test]
    fn plain_terminals_are_not_detected() {
        assert!(detect(env(&[("PATH", "/usr/bin"), ("TERM", "xterm-256color")])).is_none());
        // Orca without a reachable CLI cannot be typed into.
        assert!(detect(env(&[("ORCA_TAB_ID", "t1"), ("PATH", "")])).is_none());
    }

    #[test]
    fn records_round_trip_as_tagged_json() {
        let t = Terminal::Orca { cli: "orca".into(), tab_id: "t1".into(), pane_key: Some("t1:leaf".into()) };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"kind\":\"orca\""));
        assert_eq!(serde_json::from_str::<Terminal>(&json).unwrap(), t);
    }
}
