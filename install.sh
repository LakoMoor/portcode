#!/bin/sh
# PortCode installer for macOS and Linux (also works inside WSL).
#   curl -fsSL https://raw.githubusercontent.com/LakoMoor/portcode/main/install.sh | sh
# Options (environment):
#   PORTCODE_VERSION=v0.1.0      install a specific release (default: latest)
#   PORTCODE_INSTALL_DIR=DIR     where to put the binary (default: ~/.local/bin)
#   PORTCODE_FROM_SOURCE=1       build with cargo instead of downloading a binary
#   PORTCODE_DOWNLOAD_URL=URL    use another base URL for assets (mirrors, testing)
# Flags: --uninstall   --with-engines core|all|ids (Capstone, Unicorn, Ghidra, ImHex… into <install dir>/engines)   --with-skill DIR   (export the rustport skill, e.g. ~/.claude/skills)
set -eu

REPO="LakoMoor/portcode"
DIR="${PORTCODE_INSTALL_DIR:-$HOME/.local/bin}"
say() { printf '%s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1; }

SKILL_DIR=""
ENGINES=""
while [ $# -gt 0 ]; do
  case "$1" in
    --uninstall) rm -f "$DIR/portcode"; say "removed $DIR/portcode (config in ~/.config/portcode and project .portcode/ dirs were left alone)"; exit 0 ;;
    --with-engines) [ $# -ge 2 ] || die "--with-engines needs core, all or ids (comma separated)"; ENGINES="$2"; shift ;;
    --with-skill) [ $# -ge 2 ] || die "--with-skill needs a directory"; SKILL_DIR="$2"; shift ;;
    -h|--help) sed -n '2,11p' "$0" 2>/dev/null || true; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
  shift
done

case "$(uname -s)" in
  Darwin) os="apple-darwin" ;;
  Linux)  os="unknown-linux-gnu" ;;
  *) die "unsupported OS $(uname -s). On Windows run in PowerShell: irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex   (or: cargo install --git https://github.com/$REPO portcode-cli)" ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
  *) die "unsupported CPU $(uname -m)" ;;
esac
target="$arch-$os"

from_source() {
  need cargo || die "no prebuilt binary for $target and cargo is not installed (https://rustup.rs)"
  say "building from source with cargo (this takes a few minutes)..."
  cargo install --locked --git "https://github.com/$REPO" portcode-cli --root "${TMPDIR:-/tmp}/portcode-root.$$"
  mkdir -p "$DIR"; cp "${TMPDIR:-/tmp}/portcode-root.$$/bin/portcode" "$DIR/portcode"; rm -rf "${TMPDIR:-/tmp}/portcode-root.$$"
}

fetch() { # url dest
  if need curl; then curl -fsSL --retry 3 -o "$2" "$1"; elif need wget; then wget -q -O "$2" "$1"; else die "need curl or wget"; fi
}

sha_of() {
  if need sha256sum; then sha256sum "$1" | cut -d' ' -f1; elif need shasum; then shasum -a 256 "$1" | cut -d' ' -f1; else die "need sha256sum or shasum to verify the download"; fi
}

if [ "${PORTCODE_FROM_SOURCE:-0}" = "1" ]; then
  from_source
else
  if [ -n "${PORTCODE_DOWNLOAD_URL:-}" ]; then base="$PORTCODE_DOWNLOAD_URL"
  elif [ -n "${PORTCODE_VERSION:-}" ]; then base="https://github.com/$REPO/releases/download/$PORTCODE_VERSION"
  else base="https://github.com/$REPO/releases/latest/download"; fi
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT INT TERM
  asset="portcode-$target.tar.gz"
  say "downloading ${asset}..."
  if fetch "$base/$asset" "$tmp/$asset" && fetch "$base/$asset.sha256" "$tmp/$asset.sha256"; then
    want="$(cut -d' ' -f1 < "$tmp/$asset.sha256")"
    got="$(sha_of "$tmp/$asset")"
    [ "$want" = "$got" ] || die "checksum mismatch for $asset (expected $want, got $got) — refusing to install"
    tar -xzf "$tmp/$asset" -C "$tmp"
    [ -f "$tmp/portcode" ] || die "archive did not contain the portcode binary"
    mkdir -p "$DIR"
    install -m 755 "$tmp/portcode" "$DIR/portcode"
    say "checksum verified"
  else
    say "no prebuilt release found for $target; falling back to a source build"
    from_source
  fi
fi

say "installed: $DIR/portcode ($("$DIR/portcode" --version 2>/dev/null || echo unknown version))"
if [ -n "$ENGINES" ]; then "$DIR/portcode" engines install $(printf %s "$ENGINES" | tr , " ") || echo "note: some engines failed; see: portcode engines list" >&2; fi
if [ -n "$SKILL_DIR" ]; then "$DIR/portcode" skill export "$SKILL_DIR"; fi
case ":$PATH:" in *":$DIR:"*) ;; *) say "note: $DIR is not on your PATH. Add:  export PATH=\"$DIR:\$PATH\"" ;; esac
say "next:  portcode init  &&  portcode doctor  &&  portcode"
