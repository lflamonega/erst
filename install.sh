#!/bin/sh
# Install the latest release of erst.
#
#   curl -fsSL https://raw.githubusercontent.com/lflamonega/erst/develop/install.sh | sh
set -eu

REPO="lflamonega/erst"
BIN_DIR="${ERST_INSTALL_DIR:-/usr/local/bin}"

say() { printf 'erst: %s\n' "$1"; }
fail() { printf 'erst: error: %s\n' "$1" >&2; exit 1; }

# --- platform -----------------------------------------------------------
os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"
case "$os" in
  linux | darwin) ;;
  *) fail "unsupported operating system: $os" ;;
esac
case "$arch" in
  x86_64 | amd64) arch="x86_64" ;;
  aarch64 | arm64) arch="aarch64" ;;
  *) fail "unsupported architecture: $arch" ;;
esac
target="${arch}-unknown-${os}"
[ "$os" = "darwin" ] && target="${arch}-apple-darwin"

# --- latest release -----------------------------------------------------
say "fetching latest release"
asset="erst-${target}.tar.gz"
url="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep -o '"browser_download_url": *"[^"]*'"$asset"'"' \
  | head -n 1 \
  | cut -d '"' -f 4)"
[ -n "$url" ] || fail "no release found for ${target} (run: cargo install erst)"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading ${asset}"
curl -fsSL -o "$tmp/$asset" "$url"
tar -xzf "$tmp/$asset" -C "$tmp"

# --- install ------------------------------------------------------------
if [ -w "$BIN_DIR" ]; then
  install -m 755 "$tmp/erst" "$BIN_DIR/erst"
else
  say "${BIN_DIR} is not writable, trying sudo"
  sudo install -m 755 "$tmp/erst" "$BIN_DIR/erst"
fi

say "installed $( "$BIN_DIR/erst" --version )"
say "try: erst catalog"
