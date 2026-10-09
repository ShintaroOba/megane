---
name: megane
description: "MEGANE (眼鏡) — open the current Claude Code session in a browser tab next to the terminal, with Markdown, tables, highlighted code, Mermaid diagrams, HTML and images rendered; screenshots pasted there reach Claude with the next prompt. Use when the user says things like 'open megane', 'show this in the browser', 'this is hard to read in the terminal', 'show me a diagram', 'I want to paste a screenshot' — or in Japanese 「megane を開いて」「眼鏡」「ブラウザで見たい」「ターミナルだと読みづらい」「図で見たい」「スクショを貼りたい」. After opening it, write diagrams as Mermaid."
allowed-tools: Bash(megane:*)
argument-hint: "[session-id]"
---

# MEGANE

A lightweight viewer for the Claude Code session running in this terminal. It follows the
session's transcript (`~/.claude/projects/<project>/<session>.jsonl`) and renders it in a
browser tab: Markdown, tables, syntax-highlighted code, ```` ```mermaid ```` diagrams,
```` ```html ```` / ```` ```svg ```` blocks in a sandboxed iframe, and images. It never
touches the terminal, so session managers such as Orca keep working as before.

## Step 1: open it

`megane` starts its own background server if needed, opens the browser and returns right
away (it does not block). Calling it again only opens the browser.

```bash
megane                 # no argument: this session ($CLAUDE_CODE_SESSION_ID)
megane open <session-id>
```

Run the first line when `$ARGUMENTS` is empty, the second when a session id was given.
There is no need to run it in the background. Tell the user the URL it prints
(`http://127.0.0.1:4317/s/...`) as is.

If the command is not found (`megane: command not found`), install the prebuilt binary
with the bundled script (no Rust toolchain needed). Do not guess other commands.

```bash
# macOS / Linux
sh "${CLAUDE_PLUGIN_ROOT}/scripts/install.sh"
# Windows (from Git Bash, call PowerShell)
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/install.ps1"
```

- The script ends with `Installed: <path>`. The current shell's PATH does not pick it up,
  so use that path instead of `megane` for the rest of this session (new terminals can
  call `megane`).
- When `${CLAUDE_PLUGIN_ROOT}` is not expanded (the skill was installed into a skills
  directory rather than as a Claude Code plugin), fetch the same script from GitHub:
  `curl -fsSL https://raw.githubusercontent.com/ShintaroOba/megane/main/scripts/install.sh | sh`
  (Windows: `irm https://raw.githubusercontent.com/ShintaroOba/megane/main/scripts/install.ps1 | iex`).
- With a Rust toolchain, `cargo install --git https://github.com/ShintaroOba/megane` also works.

## Step 2: write for the browser from now on

While MEGANE is open, this session's replies are rendered as HTML. Follow these rules.

- Diagrams go in ```` ```mermaid ```` fences (flowchart / sequenceDiagram / classDiagram /
  stateDiagram-v2 / erDiagram / gantt / mindmap). Never draw diagrams in ASCII art or box
  characters.
- A visual that Mermaid cannot express (a styled mockup, a chart) can go in an
  ```` ```html ```` or ```` ```svg ```` fence; it renders in a sandboxed iframe.
- Comparisons and lists of options go in Markdown tables.
- Split long explanations with `##` headings.
- Give code blocks a language (```` ```rust ````, ```` ```bash ````, ```` ```json ```` …).
- To show an image file on disk, link it with its absolute path: `![caption](C:\path\shot.png)`.
- No terminal-style alignment (full-width spaces, ruled boxes).

## Answers from the browser

When the user answers an AskUserQuestion in the viewer, the tool result carries the answers
as usual. A plan rejected or a permission denied in the viewer comes back as a denial whose
reason starts with "The user rejected the plan in the MEGANE viewer" or "The user denied this in
the MEGANE viewer", followed by the user's instructions if any. Follow those instructions; do not
retry the same call unchanged.

## Screenshots from the user

The user can paste (Ctrl+V) or drop an image into the viewer. It is saved under
`~/.megane/inbox/<session>/`, and the plugin's `UserPromptSubmit` hook adds a note to the
next prompt listing the saved paths. When such a note appears, read those files with the
Read tool before answering. Without the plugin's hook, the user pastes the path (already
copied to the clipboard) into the prompt instead.

## When the user asks how it works

- Prompts are still typed in the terminal. The viewer can answer what Claude asks the user:
  AskUserQuestion, ExitPlanMode plan approvals and permission prompts (the plugin's
  PreToolUse / PermissionRequest hooks). That happens only while a viewer tab is visible and
  its "ブラウザで回答" toggle is on; otherwise, or when the user presses "ターミナルで答える" on
  the card, Claude Code asks in the terminal as usual. For a browser that also sends prompts,
  point to OYAKATA (https://github.com/ShintaroOba/oyakata).
- `megane open --list` opens the list of all sessions; `megane open --print` prints the
  URL without launching a browser, for opening it from Orca or another session manager.
- The server listens on `127.0.0.1:4317` (`--port` or `MEGANE_PORT` to change it) and reads
  the transcripts only. The rendering libraries load from cdn.jsdelivr.net.
- The page follows the session live; the user never needs to reload.
