<div align="center">

<img src="web/icon.svg" width="96" height="96" alt="MEGANE">

# MEGANE（眼鏡）

**Claude Code のターミナルに、眼鏡を。**

ターミナルで動いている Claude Code のセッションを、隣のブラウザタブで描画します。
Markdown・表・ハイライト付きコード・Mermaid 図・HTML・画像をきれいに表示します。
ターミナル自体には触れないので、Orca などのセッション管理ツールとそのまま併用できます。

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](#ターミナルから使う)
[![Claude Code](https://img.shields.io/badge/Claude%20Code-viewer-8A2BE2.svg)](#スキルではじめる)

[English](README.md) | **日本語**

</div>

## これは何？

Claude Code はターミナルで動かすぶんには快適ですが、出力を読むのは別問題です。

- 表は折り返して崩れ、図はアスキーアートになり、HTML はソースのままになります。
- Claude が撮ったスクショは見えず、自分のスクショを Claude に渡すのもひと手間です。
- かといってターミナルを GUI アプリに置き換えると、`claude` を管理してくれている Orca などのツールと噛み合わなくなります。

MEGANE はターミナルをそのままにして、「見る」ための目だけを足します。
Claude Code がもともと書き出している会話ログ（`~/.claude/projects/<project>/<session>.jsonl`）を読み、
追記されるたびにブラウザへ流します。データが手元の PC から出ることはありません。

| | できること |
| --- | --- |
| **読む** | Markdown・表・シンタックスハイライト付きコードを HTML として表示。ライト／ダーク両対応 |
| **見る** | ` ```mermaid ` は図に、` ```html ` / ` ```svg ` は sandbox 付き iframe で描画。画像（自分の・Claude の・ツールのスクショ）もその場で表示し、クリックで拡大 |
| **絞る** | ツール呼び出しは 1 行の要約に折りたたみ。チェックボックス 1 つで丸ごと非表示 |
| **見せる** | ビューアに Ctrl+V かドラッグ＆ドロップでスクショを入れると、次のプロンプトで Claude に渡る |

## スキルではじめる

MEGANE は Claude Code のプラグインとして配布しています。入れたら Claude に「megane を開いて」と頼むだけで、
あとは Claude がやってくれます。Rust のツールチェーンは要りません。

**1. プラグインを入れる**

Claude Code で次のように打ちます。

```
/plugin install megane --marketplace ShintaroOba/megane
```

Claude Code 2.1.275 より古い場合は、先にマーケットプレイスを追加します。

```
/plugin marketplace add ShintaroOba/megane
/plugin install megane@megane
```

ターミナルからなら `claude plugin marketplace add ShintaroOba/megane` のあと `claude plugin install megane@megane` です。

**2. 呼び出す**

```
/megane
```

「megane を開いて」「ブラウザで見たい」「ターミナルだと読みづらい」といった言い方でもスキルが動きます。

`megane` バイナリが無ければ、Claude が同梱のスクリプトで [GitHub Releases](https://github.com/ShintaroOba/megane/releases)
からビルド済みのものを入れ（Windows / macOS / Linux）、今のセッションをブラウザで開きます。
それ以降、Claude はブラウザで読まれている前提で、図は Mermaid、比較は表で書くようになります。

**3. スクショを貼る**

ビューアに画像を貼る（Ctrl+V）かドロップすると `~/.megane/inbox/<session>/` に保存され、
次にターミナルでプロンプトを送ったとき、プラグインの `UserPromptSubmit` hook が「この画像を読んで」と Claude に伝えます。
保存先のパスはクリップボードにも入るので、自分でプロンプトに貼っても渡せます。
セッションの途中でバイナリを入れた場合も、hook がインストール先から見つけるので再起動は要りません。

## ターミナルから使う

MEGANE はスキルなしでも、普通のコマンドとして使えます。実行時の依存がない約 1 MB の単一バイナリです。

### バイナリを入れる

ビルド済みバイナリ（Windows x64 / arm64、macOS Intel / Apple Silicon、Linux x64 / arm64）を
[GitHub Release](https://github.com/ShintaroOba/megane/releases) ごとに添付しています。
インストールスクリプトがお使いの環境向けのものを取得し、SHA-256 を検証して PATH に置きます。

```bash
# macOS / Linux: ~/.local/bin に入ります
curl -fsSL https://raw.githubusercontent.com/ShintaroOba/megane/main/scripts/install.sh | sh
```

```powershell
# Windows: %LOCALAPPDATA%\Programs\megane に入れ、ユーザーの PATH に追加します
irm https://raw.githubusercontent.com/ShintaroOba/megane/main/scripts/install.ps1 | iex
```

`MEGANE_INSTALL_DIR` で置き場所を、`MEGANE_VERSION=v0.1.0` でバージョンを指定できます。
Rust のツールチェーンがあれば `cargo install --git https://github.com/ShintaroOba/megane` でも入ります。

### セッションを開く

Claude Code の入力欄で `! megane` と打つと、モデルを介さずにコマンドが実行され、今いるセッションが開きます
（Claude Code が `$CLAUDE_CODE_SESSION_ID` で渡してくれます）。

```bash
megane                   # 今のセッションを開く（必要ならサーバーを起動）
megane open <session>    # セッションを指定して開く
megane open --list       # セッション一覧を開く
megane open --print      # ブラウザを起動せず URL だけ表示
megane serve             # サーバーをフォアグラウンドで起動
```

セッション ID を省略すると、`$CLAUDE_CODE_SESSION_ID`、次にカレントディレクトリで最後に更新されたセッションを使います。

| オプション | 既定値 | 意味 |
| --- | --- | --- |
| `--port <n>` / `MEGANE_PORT` | `4317` | 待ち受けポート |
| `CLAUDE_CONFIG_DIR` | `~/.claude` | 読み取る Claude Code の設定ディレクトリ |

プラグインを使わずにスクショを Claude に渡したい場合は、`~/.claude/settings.json` に hook を自分で追加します。

```json
{
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [{ "type": "command", "command": "megane hook prompt" }] }
    ]
  }
}
```

### Orca などのセッション管理ツールと使う

MEGANE は `claude` のプロセスもターミナルも包まないので、セッションの管理はこれまでどおりツール側に任せられます。
セッションごとに `http://127.0.0.1:4317/s/<session-id>` という固定の URL があります。
`megane open --print <session-id>` で取得して、ツールのブラウザペインやリンクから開いてください。

## FAQ

**ブラウザからプロンプトを送れますか？**

送れません。MEGANE は読むだけで、入力はターミナルで行います。ブラウザからセッションを操作したい場合は
[OYAKATA](https://github.com/ShintaroOba/oyakata) を使ってください。

**データはどこかに送られますか？**

送られません。サーバーは `127.0.0.1` にだけバインドし、Claude Code が書いた会話ログを読むだけです。
ただし描画ライブラリ（marked、DOMPurify、highlight.js、Mermaid）は jsDelivr から読み込むため、
ブラウザから `cdn.jsdelivr.net` に接続できる必要があります。

**Claude に図を描かせるには？**

`~/.claude/CLAUDE.md` に次のような 1 行を足します。

```
Write diagrams in ```mermaid fences (MEGANE renders them in the browser). Do not draw diagrams as ASCII art.
```

<details>
<summary>仕組みと安全性</summary>

- **セッションの追従**：サーバーは 300 ms ごとに会話ログを確認し、追記された分だけを読みます。
  各行を表示用メッセージ（テキスト・画像・ツール呼び出し・ツール結果）に変換し、Server-Sent Events でブラウザへ送ります。
  サブエージェント（sidechain）とメタ情報のレコードは読み飛ばし、ユーザー発言内の `<system-reminder>` は取り除きます。
  ツール出力は 4,000 文字で切ります。
- **セッション一覧**：タイトルは最新の `ai-title` レコードから取ります。各ログの末尾 256 KB だけを読むので、一覧表示は軽いままです。
- **貼り付けた画像**は `~/.megane/inbox/<session>/` に保存されます。`megane hook prompt` がそのパスを Claude 向けに出力し
  （`UserPromptSubmit` hook の出力はコンテキストに入ります）、`sent/` へ移すので、1 枚の画像が渡るのは 1 回だけです。
- **HTML / SVG ブロック**は `<iframe sandbox="allow-scripts">` で描画します。スクリプトは動きますが、ビューアのオリジンには触れません。
  Markdown は DOMPurify で無害化し、Mermaid は `securityLevel: strict` で動かします。
- **ローカル画像**（`![](C:\path\shot.png)` のようなリンク）は `/api/file` 経由で配信します。画像の拡張子に限り、
  `Content-Security-Policy: sandbox` を付けます。
- **ネットワーク**：`Host` が `127.0.0.1` / `localhost` 以外のリクエストは拒否し（DNS リバインディング対策）、
  書き込み系のリクエストには同一オリジンの `Origin` ヘッダを求めます。

</details>

## 開発

```bash
cargo test
cargo run -- serve
```

Windows で `stable-x86_64-pc-windows-gnu` ツールチェーンを使う場合、`windows-sys` のビルドに MinGW-w64 一式（`dlltool` と `as`）が必要です。
たとえば `winget install BrechtSanders.WinLibs.POSIX.UCRT` で入れ、ビルド時に `mingw64\bin` を `PATH` に通してください。
MSVC ツールチェーンなら追加のものは要りません。

リリースするときは、`Cargo.toml` と `.claude-plugin/plugin.json` の `version` を同じ番号に上げ、`vX.Y.Z` タグを push します。
`.github/workflows/release.yml` が 6 ターゲットをビルドして GitHub Release に添付し、インストールスクリプトはそこから取得します。

```bash
git tag v0.1.0 && git push origin v0.1.0
```

```
src/
  main.rs        CLI（open / serve / hook）とバックグラウンドサーバーの起動
  server.rs      axum のルーティング、SSE での追従、画像の受け取り、ローカルファイル配信、リクエスト検査
  transcript.rs  セッションの特定、タイトル、JSONL → 表示用メッセージ
  hook.rs        UserPromptSubmit hook と画像の inbox
web/
  app.html       ビューア本体（セッション一覧と会話）。バイナリに埋め込み
  icon.svg       アイコン
skills/megane/   /megane スキル
hooks/           プラグインの UserPromptSubmit hook（scripts/hook.sh を実行）
scripts/         install.sh / install.ps1（GitHub Releases からビルド済みバイナリを取得）、hook.sh
.claude-plugin/  プラグインとマーケットプレイスのマニフェスト
.github/workflows/release.yml  バージョンタグで 6 ターゲットをビルドしてリリースを公開
```

## ライセンス

MIT
