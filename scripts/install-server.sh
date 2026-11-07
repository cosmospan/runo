#!/bin/sh
# Puts Squad Code on a Linux server and keeps `squad serve` running there, so the
# crew carries on with your laptop closed. The desktop app runs this over SSH
# when you connect it to a server; you can also run it yourself:
#
#     curl -fsSL https://raw.githubusercontent.com/hyperhale/squadcode/main/scripts/install-server.sh | sh
#
# It installs into ~/.local/bin without root. Only the few libraries the binary
# links against need root, and it asks for them with `sudo -n` when they're
# missing, which never prompts. The last line it prints is `bin=<path>`.
set -eu

REPO="hyperhale/squadcode"
dir="$HOME/.local/bin"
bin="$dir/SquadCode"

say() { printf '%s\n' "$*"; }
fail() { say "error: $*" >&2; exit 1; }

[ "$(uname -s)" = Linux ] || fail "this server runs $(uname -s); squad serve needs Linux."

case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *) fail "no Squad Code build for $(uname -m) yet." ;;
esac

# Already there (installed before, or put on the PATH by hand): use it.
if [ ! -x "$bin" ] && command -v SquadCode >/dev/null 2>&1; then
  bin=$(command -v SquadCode)
fi

if [ ! -x "$bin" ] || [ "${SQUAD_UPDATE:-}" = 1 ]; then
  command -v curl >/dev/null 2>&1 || fail "curl isn't installed here."
  say "Finding the latest Squad Code for Linux $arch…"
  url=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | grep -o "\"browser_download_url\": *\"[^\"]*linux-$arch[^\"]*\.tar\.gz\"" \
    | head -1 | sed 's/.*"\(https[^"]*\)"$/\1/') || true
  [ -n "${url:-}" ] || fail "no Linux release of Squad Code is published yet. Copy a SquadCode binary to $bin and connect again."
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  curl -fsSL "$url" -o "$tmp/squad.tar.gz" || fail "couldn't download $url"
  tar -xzf "$tmp/squad.tar.gz" -C "$tmp"
  found=$(find "$tmp" -type f -name SquadCode | head -1)
  [ -n "$found" ] || fail "the download didn't have a SquadCode binary in it."
  mkdir -p "$dir"
  bin="$dir/SquadCode"
  install -m 755 "$found" "$bin"
  say "Installed $bin."
fi

# The binary links a few window-system libraries even when it serves without a
# window. Add the missing ones when we can do it without a password.
if command -v ldd >/dev/null 2>&1 && ldd "$bin" 2>/dev/null | grep -q "not found"; then
  if command -v apt-get >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    say "Adding the libraries Squad Code needs…"
    sudo -n apt-get update -qq >/dev/null 2>&1 || true
    sudo -n apt-get install -y -qq libxcb1 libxkbcommon0 libxkbcommon-x11-0 libxcb-xkb1 >/dev/null 2>&1 || true
  fi
  missing=$(ldd "$bin" 2>/dev/null | grep "not found" | awk '{print $1}' | tr '\n' ' ')
  [ -z "$missing" ] || fail "Squad Code needs these libraries: $missing. Install them (for Ubuntu: sudo apt-get install libxcb1 libxkbcommon0 libxkbcommon-x11-0) and connect again."
fi

"$bin" v2 serve --install
say "bin=$bin"
