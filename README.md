<div align="center">

<img src="web/icon.svg" width="96" height="96" alt="MEGANE">

# MEGANE（眼鏡）

**Put on glasses for your Claude Code terminal.**

MEGANE renders the Claude Code session you are running in a terminal — Markdown, tables, highlighted code,
Mermaid diagrams, HTML and images — in a browser tab next to it. It never touches the terminal itself,
so it fits right in with Orca and other session managers.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](#from-the-terminal)
[![Claude Code](https://img.shields.io/badge/Claude%20Code-viewer-8A2BE2.svg)](#get-started)

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

## Get started

**1. Install**

```bash
cargo install --git https://github.com/ShintaroOba/megane
```

This needs a Rust toolchain. On Windows with the GNU toolchain, see [Development](#development).

**2. Open it from your session**

In Claude Code, type:

```
! megane
```

The `!` prefix runs the command directly, without a model turn. MEGANE starts its background server
if needed and opens the session you are in (Claude Code passes it as `$CLAUDE_CODE_SESSION_ID`).

**3. (Optional) Hand screenshots to Claude**

Add a `UserPromptSubmit` hook to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [{ "type": "command", "command": "megane hook prompt" }] }
    ]
  }
}
```

Now paste or drop an image into the viewer. It is saved under `~/.megane/inbox/<session>/`,
and the next prompt you send in the terminal tells Claude to read it. Without the hook,
the viewer still copies the saved path to your clipboard so you can paste it into the prompt.

## From the terminal

MEGANE is a single binary of about 1 MB.

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

### With Orca and other session managers

MEGANE does not wrap the `claude` process or its terminal, so a session manager keeps owning it.
Every session has a stable URL, `http://127.0.0.1:4317/s/<session-id>`. Get it with
`megane open --print <session-id>` and open it wherever your manager can show a browser pane or link.

## FAQ

**Can I send prompts from the browser?**

No. MEGANE only reads; you keep typing in the terminal. If you want a browser command post that also
drives sessions, see [OYAKATA](https://github.com/ShintaroOba/oyakata).

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
- **Network.** Requests whose `Host` is not `127.0.0.1` / `localhost` are refused (DNS rebinding),
  and writes need a same-origin `Origin` header.

</details>

## Development

```bash
cargo build --release
cargo run -- serve
```

On Windows with the `stable-x86_64-pc-windows-gnu` toolchain, `windows-sys` needs a full MinGW-w64
(`dlltool` and `as`). Install one, for example `winget install BrechtSanders.WinLibs.POSIX.UCRT`,
and put its `mingw64\bin` on `PATH` while building. The MSVC toolchain needs nothing extra.

```
src/
  main.rs        CLI (open, serve, hook) and starting the background server
  server.rs      axum routes, SSE tailing, image paste, local file serving, request guard
  transcript.rs  locating sessions, titles, JSONL → display messages
  hook.rs        UserPromptSubmit hook and the image inbox
web/
  app.html       the viewer (session list and conversation), embedded into the binary
  icon.svg       the icon
```

## License

MIT
