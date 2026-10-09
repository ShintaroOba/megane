#!/bin/sh
# Installs the megane binary from GitHub Releases. No Rust toolchain needed.
#
#   sh scripts/install.sh [version]      # default: latest; e.g. sh scripts/install.sh v0.1.0
#   curl -fsSL https://raw.githubusercontent.com/ShintaroOba/megane/main/scripts/install.sh | sh
#
# Environment:
#   MEGANE_INSTALL_DIR  where the binary goes (default: ~/.local/bin)
#   MEGANE_VERSION      release tag to install (default: latest)
#   MEGANE_REPO         GitHub repository (default: ShintaroOba/megane)
#   MEGANE_BASE_URL     download the archive from here instead of GitHub (a mirror or file:// URL)
#   MEGANE_TARGET       force a target triple instead of detecting it (e.g. x86_64-unknown-linux-musl)
#
# Windows: use scripts/install.ps1 instead.
set -eu

repo=${MEGANE_REPO:-ShintaroOba/megane}
version=${1:-${MEGANE_VERSION:-latest}}
dir=${MEGANE_INSTALL_DIR:-$HOME/.local/bin}

die() { printf 'megane install: %s\n' "$*" >&2; exit 1; }

if [ -n "${MEGANE_TARGET:-}" ]; then
  target=$MEGANE_TARGET
else
  os=$(uname -s)
  arch=$(uname -m)
  case "$os" in
    Linux) suffix=unknown-linux-musl ;;
    Darwin) suffix=apple-darwin ;;
    MINGW*|MSYS*|CYGWIN*|Windows*) die "on Windows run: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/install.ps1" ;;
    *) die "unsupported OS: $os" ;;
  esac
  case "$arch" in
    x86_64|amd64) cpu=x86_64 ;;
    aarch64|arm64) cpu=aarch64 ;;
    *) die "unsupported CPU: $arch" ;;
  esac
  target="$cpu-$suffix"
fi
asset="megane-$target.tar.gz"

if [ -n "${MEGANE_BASE_URL:-}" ]; then
  base=${MEGANE_BASE_URL%/}
elif [ "$version" = latest ]; then
  base="https://github.com/$repo/releases/latest/download"
else
  base="https://github.com/$repo/releases/download/$version"
fi

fetch() { # url dest
  if command -v curl > /dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget > /dev/null 2>&1; then
    wget -qO "$2" "$1"
  else
    die "curl or wget is required"
  fi
}

sha256() {
  if command -v sha256sum > /dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum > /dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
  else echo ""
  fi
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

printf 'Downloading %s/%s\n' "$base" "$asset"
fetch "$base/$asset" "$tmp/$asset" || die "download failed (no release for $target at $base?)"

if fetch "$base/$asset.sha256" "$tmp/$asset.sha256" 2> /dev/null; then
  expected=$(cut -d' ' -f1 "$tmp/$asset.sha256")
  actual=$(sha256 "$tmp/$asset")
  if [ -n "$actual" ] && [ "$expected" != "$actual" ]; then
    die "checksum mismatch for $asset (expected $expected, got $actual)"
  fi
else
  printf 'warning: no checksum published for %s, skipping verification\n' "$asset" >&2
fi

tar -xzf "$tmp/$asset" -C "$tmp"
bin=$(find "$tmp" -type f -name megane | head -n 1)
[ -n "$bin" ] || die "the archive did not contain an megane binary"

mkdir -p "$dir"
# Copy next to the destination and rename, so a running server keeps its old file
# and the switch is atomic.
cp "$bin" "$dir/megane.new"
chmod 755 "$dir/megane.new"
mv -f "$dir/megane.new" "$dir/megane"

"$dir/megane" --version
printf 'Installed: %s\n' "$dir/megane"

case ":$PATH:" in
  *":$dir:"*) ;;
  *)
    printf '\n%s is not on your PATH. Add it to your shell profile, for example:\n' "$dir" >&2
    printf '  export PATH="%s:$PATH"\n' "$dir" >&2
    ;;
esac
other=$(command -v megane 2> /dev/null || true)
if [ -n "$other" ] && [ "$other" != "$dir/megane" ]; then
  printf '\nNote: `megane` currently resolves to %s, which comes earlier on PATH.\n' "$other" >&2
fi
