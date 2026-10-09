<div align="center">

<img src="web/icon.svg" width="96" height="96" alt="MEGANE">

# MEGANE（眼鏡）

**Put on glasses for your Claude Code terminal.**

MEGANE renders the Claude Code session you are running in a terminal — Markdown, tables, highlighted code,
Mermaid diagrams, HTML and images — in a browser tab next to it. It never touches the terminal itself,
so it fits right in with Orca and other session managers.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](#from-the-terminal)
[![Claude Code](https://img.shields.io/badge/Claude%20Code-viewer-8A2BE2.svg)](#get-started-with-the-skill)

**English** | [日本語](README.ja.md)

</div>

## What is it?

Claude Code lives in the terminal, and the terminal is great for driving it. Reading it is another matter.

- Tables wrap, diagrams turn into ASCII art, and HTML stays HTML source.
- You cannot see a screenshot Claude took, and handing Claude your own screenshot is clumsy.
- Replacing the terminal with a GUI app breaks the session managers (Orca and friends) that run `claude` for you.

MEGANE leaves the terminal alone and adds a second pair of eyes. It reads the transcript Claude Code
already writes (`~/.claude/projects/<project>/<session>.jsonl`) and streams it to a browser tab as it grows.
Everything stays on your machine.

| | What you can do |
| --- | --- |
| **Read** | Markdown, tables and syntax-highlighted code, rendered as HTML. Light and dark themes |
| **See** | ` ```mermaid ` fences become diagrams, ` ```html ` / ` ```svg ` fences render in a sandboxed iframe, and images (yours, Claude's, tool screenshots) show inline. Click to zoom |
| **Focus** | Tool calls fold into one-line summaries. Hide them entirely with one checkbox |
| **Show** | Paste (Ctrl+V) or drop a screenshot into the viewer, and Claude gets it with your next prompt |
| **Answer** | When Claude asks a question, wants a plan approved or needs permission to run a tool, a card appears in the viewer. Pick an option, approve or send back the plan, or allow / deny the tool, right there |

## Get started with the skill

MEGANE ships as a Claude Code plugin. Install it, ask Claude to open MEGANE, and Claude does
the rest. No Rust toolchain needed.

**1. Install the plugin**

In Claude Code:

```
/plugin install megane --marketplace ShintaroOba/megane
```

On Claude Code older than 2.1.275, add the marketplace first:

```
/plugin marketplace add ShintaroOba/megane
/plugin install megane@megane
```

From a terminal: `claude plugin marketplace add ShintaroOba/megane`, then `claude plugin install megane@megane`.

**2. Ask for it**

```
/megane
```

Phrases like "open megane", "show this in the browser" or "this is hard to read in the terminal" trigger
the skill too.

If the `megane` binary is missing, Claude installs a prebuilt one from
[GitHub Releases](https://github.com/ShintaroOba/megane/releases) with the bundled script
(Windows / macOS / Linux), then opens the current session in your browser. From then on, Claude
knows it is being read in a browser: it draws diagrams as Mermaid and comparisons as tables.

**3. Paste screenshots**

Paste (Ctrl+V) or drop an image into the viewer. It is saved under `~/.megane/inbox/<session>/`,
and the plugin's `UserPromptSubmit` hook tells Claude to read it with the next prompt you send
in the terminal. The viewer also copies the saved path to your clipboard, so you can paste it
into the prompt yourself. If the binary was installed during the session, the hook finds it in
its install folder; nothing to restart.

## From the terminal

MEGANE also works as a plain command, without the skill. It is a single binary of about 1 MB with
no runtime dependencies.

### Install the binary

Prebuilt binaries (Windows x64 / arm64, macOS Intel / Apple Silicon, Linux x64 / arm64) are attached
to every [GitHub Release](https://github.com/ShintaroOba/megane/releases). The install script downloads
the one for your machine, verifies its SHA-256 and puts it on your PATH.

```bash
# macOS / Linux: installs to ~/.local/bin
curl -fsSL https://raw.githubusercontent.com/ShintaroOba/megane/main/scripts/install.sh | sh
```

```powershell
# Windows: installs to %LOCALAPPDATA%\Programs\megane and adds it to your user PATH
irm https://raw.githubusercontent.com/ShintaroOba/megane/main/scripts/install.ps1 | iex
```

`MEGANE_INSTALL_DIR` changes the location and `MEGANE_VERSION=v0.2.0` pins a release. With a Rust
toolchain, `cargo install --git https://github.com/ShintaroOba/megane` works too.

### Open a session

In Claude Code, `! megane` runs the command directly, without a model turn, and opens the session
you are in (Claude Code passes it as `$CLAUDE_CODE_SESSION_ID`).

```bash
megane                   # open the current session (start the server if needed)
megane open <session>    # open a specific session
megane open --list       # open the session list
megane open --print      # print the URL instead of launching a browser
megane serve             # run the server in the foreground
```

Without a session id, MEGANE uses `$CLAUDE_CODE_SESSION_ID`, then the most recently updated session
in the current directory.

| Option | Default | Meaning |
| --- | --- | --- |
| `--port <n>` / `MEGANE_PORT` | `4317` | Port to listen on |
| `CLAUDE_CONFIG_DIR` | `~/.claude` | Claude Code config directory to read from |

Without the plugin, add the hooks to `~/.claude/settings.json` yourself. `prompt` hands pasted screenshots to Claude; `pretool` and `permission` let the viewer answer questions, plan approvals and permission prompts:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [{ "type": "command", "command": "megane hook prompt" }] }
    ],
    "PreToolUse": [
      { "matcher": "AskUserQuestion|ExitPlanMode",
        "hooks": [{ "type": "command", "command": "megane hook pretool", "timeout": 600 }] }
    ],
    "PermissionRequest": [
      { "hooks": [{ "type": "command", "command": "megane hook permission", "timeout": 600 }] }
    ]
  }
}
```

### With Orca and other session managers

MEGANE does not wrap the `claude` process or its terminal, so a session manager keeps owning it.
Every session has a stable URL, `http://127.0.0.1:4317/s/<session-id>`. Get it with
`megane open --print <session-id>` and open it wherever your manager can show a browser pane or link.

## FAQ

**Can I send prompts from the browser?**

No. You keep typing prompts in the terminal. What the browser can do is answer what Claude asks *you*:
questions (AskUserQuestion), plan approvals (ExitPlanMode) and permission prompts.

This only happens while a viewer tab is visible and **ブラウザで回答** (answer in the browser) is on in its
header. Otherwise, and whenever you press "ターミナルで答える" (answer in the terminal) on a card, Claude Code
asks in the terminal as usual. While a card is waiting, the terminal shows nothing to answer, so look at the
browser (the tab title starts with ❓).

If you want a browser command post that also sends prompts, see [OYAKATA](https://github.com/ShintaroOba/oyakata).

**Where does my data go?**

Nowhere. The server binds to `127.0.0.1` only and reads the transcripts Claude Code writes.
The rendering libraries (marked, DOMPurify, highlight.js, Mermaid) are loaded from jsDelivr,
so the browser needs to reach `cdn.jsdelivr.net`.

**How do I make Claude draw diagrams I can see?**

Add a line like this to `~/.claude/CLAUDE.md`:

```
Write diagrams in ```mermaid fences (MEGANE renders them in the browser). Do not draw diagrams as ASCII art.
```

<details>
<summary>How it works and safety</summary>

- **Following a session.** The server polls the transcript every 300 ms and reads only the appended
  bytes. Each line becomes a display message (text, image, tool call, tool result) and is pushed to
  the browser over Server-Sent Events. Sidechain (sub-agent) and meta records are skipped, and
  `<system-reminder>` blocks are stripped from user messages. Tool output is cut at 4,000 characters.
- **Session list.** Titles come from the newest `ai-title` record, looked up in the last 256 KB of each
  transcript so listing stays cheap.
- **Pasted images** are written to `~/.megane/inbox/<session>/`. `megane hook prompt` prints their paths
  for Claude (a `UserPromptSubmit` hook's output becomes context) and moves them to `sent/`, so each
  image is handed over once.
- **HTML and SVG fences** render in an `<iframe sandbox="allow-scripts">`: scripts run, but without access
  to the viewer's origin. Markdown is sanitised with DOMPurify, and Mermaid runs with `securityLevel: strict`.
- **Local images** that a message links to (`![](C:\path\shot.png)`) are served through `/api/file`,
  for image extensions only, with `Content-Security-Policy: sandbox`.
- **Answering from the browser** uses documented hooks, not keystrokes. `megane hook pretool` (PreToolUse on
  AskUserQuestion and ExitPlanMode) and `megane hook permission` (PermissionRequest) send the prompt to the
  server and wait. The server forwards it to the viewers, and returns the answer when one comes. The hook then
  prints `permissionDecision: allow` with `updatedInput.answers`, a deny with your message, or a
  PermissionRequest `decision`. When no viewer is ready, the viewer closes, you hand it back, or 9 minutes
  pass, the hook prints nothing and Claude Code asks in the terminal. "Always allow" applies the
  `permission_suggestions` Claude Code offers.
- **Network.** Requests whose `Host` is not `127.0.0.1` / `localhost` are refused (DNS rebinding),
  and writes need a same-origin `Origin` header.

</details>

## Development

```bash
cargo test
cargo run -- serve
```

On Windows with the `stable-x86_64-pc-windows-gnu` toolchain, `windows-sys` needs a full MinGW-w64
(`dlltool` and `as`). Install one, for example `winget install BrechtSanders.WinLibs.POSIX.UCRT`,
and put its `mingw64\bin` on `PATH` while building. The MSVC toolchain needs nothing extra.

To release, bump `version` in `Cargo.toml` and `.claude-plugin/plugin.json` to the same number and
push a `vX.Y.Z` tag. `.github/workflows/release.yml` builds six targets and attaches the archives to a
GitHub Release, which is where the install scripts download from.

```bash
git tag v0.2.0 && git push origin v0.2.0
```

```
src/
  main.rs        CLI (open, serve, hook) and starting the background server
  server.rs      axum routes, SSE tailing, image paste, local file serving, request guard
  transcript.rs  locating sessions, titles, JSONL → display messages
  hook.rs        UserPromptSubmit hook and the image inbox
web/
  app.html       the viewer (session list and conversation), embedded into the binary
  icon.svg       the icon
skills/megane/   the /megane skill
hooks/           the plugin's UserPromptSubmit hook (runs scripts/hook.sh)
scripts/         install.sh / install.ps1 (download a prebuilt binary from GitHub Releases), hook.sh
.claude-plugin/  plugin and marketplace manifests
.github/workflows/release.yml  builds six targets on a version tag and publishes the release
```

## License

MIT
