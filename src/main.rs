mod bridge;
mod hook;
mod live;
mod server;
mod transcript;

use std::net::{SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

const DEFAULT_PORT: u16 = 4317;

#[derive(Parser)]
#[command(name = "megane", version, about = "Render Claude Code sessions next to your terminal")]
struct Cli {
    /// Port of the local viewer server (also MEGANE_PORT)
    #[arg(long, global = true, env = "MEGANE_PORT", default_value_t = DEFAULT_PORT)]
    port: u16,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Open a session in the browser (default). Starts the server if needed.
    Open {
        /// Session id. Defaults to $CLAUDE_CODE_SESSION_ID, then the latest session in the current directory.
        session: Option<String>,
        /// Open the session list instead of a single session
        #[arg(long)]
        list: bool,
        /// Print the URL without launching a browser
        #[arg(long)]
        print: bool,
        /// Use the system browser even inside a terminal app with its own (Orca)
        #[arg(long)]
        browser: bool,
    },
    /// Run the viewer server in the foreground
    Serve,
    /// Claude Code hook entry points
    Hook {
        #[command(subcommand)]
        kind: HookKind,
    },
}

#[derive(Subcommand)]
enum HookKind {
    /// SessionStart: remember which terminal the session runs in
    Start,
    /// UserPromptSubmit: hand images pasted into the viewer to Claude
    Prompt,
    /// PreToolUse (AskUserQuestion, ExitPlanMode): let the viewer answer
    Pretool,
    /// PermissionRequest: let the viewer allow or deny
    Permission,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Cmd::Open { session: None, list: false, print: false, browser: false }) {
        Cmd::Serve => tokio::runtime::Runtime::new()?.block_on(server::serve(cli.port)),
        Cmd::Hook { kind: HookKind::Start } => hook::start(),
        Cmd::Hook { kind: HookKind::Prompt } => hook::prompt(),
        Cmd::Hook { kind: HookKind::Pretool } => hook::ask(cli.port, hook::Ask::PreTool),
        Cmd::Hook { kind: HookKind::Permission } => hook::ask(cli.port, hook::Ask::Permission),
        Cmd::Open { session, list, print, browser } => open(cli.port, session, list, print, browser),
    }
}

fn open(port: u16, session: Option<String>, list: bool, print: bool, browser: bool) -> Result<()> {
    // Run from inside Claude Code (`! megane`), we can see which terminal this session is in.
    if let Ok(current) = std::env::var("CLAUDE_CODE_SESSION_ID") {
        bridge::record(&current);
    }
    let path = if list {
        "/".to_string()
    } else {
        let id = session
            .or_else(|| std::env::var("CLAUDE_CODE_SESSION_ID").ok().filter(|s| !s.is_empty()))
            .or_else(|| {
                let cwd = std::env::current_dir().ok()?;
                transcript::latest_for_cwd(&cwd).map(|s| s.id)
            });
        match id {
            Some(id) => format!("/s/{id}"),
            None => "/".to_string(),
        }
    };

    ensure_server(port)?;
    let url = format!("http://127.0.0.1:{port}{path}");
    println!("{url}");
    let in_app = !print && !browser
        && bridge::detect(|k| std::env::var(k).ok()).is_some_and(|t| bridge::open_url(&t, &url));
    if !print && !in_app {
        if let Err(e) = open::that_detached(&url) {
            eprintln!("could not launch a browser ({e}); open the URL above yourself");
        }
    }
    Ok(())
}

fn server_up(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

fn ensure_server(port: u16) -> Result<()> {
    if server_up(port) {
        return Ok(());
    }
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.args(["--port", &port.to_string(), "serve"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        // Own process group, so the server outlives the shell that started it.
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().context("failed to start megane server")?;

    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(100));
        if server_up(port) {
            return Ok(());
        }
    }
    bail!("megane server did not start on port {port}")
}
