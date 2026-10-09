mod hook;
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
        /// Print the URL without launching a browser (for Orca or other session managers)
        #[arg(long)]
        print: bool,
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
    /// UserPromptSubmit: hand images pasted into the viewer to Claude
    Prompt,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Cmd::Open { session: None, list: false, print: false }) {
        Cmd::Serve => tokio::runtime::Runtime::new()?.block_on(server::serve(cli.port)),
        Cmd::Hook { kind: HookKind::Prompt } => hook::prompt(),
        Cmd::Open { session, list, print } => open(cli.port, session, list, print),
    }
}

fn open(port: u16, session: Option<String>, list: bool, print: bool) -> Result<()> {
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
    if !print {
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
