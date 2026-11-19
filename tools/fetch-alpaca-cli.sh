#!/bin/sh
# Fetch the pinned Alpaca CLI release into binaries/ for bundling.
#
#   sh tools/fetch-alpaca-cli.sh
#
# The CLI ships as an application resource, not an externalBin: Tauri installs
# externalBin to /usr/bin, which would put `alpaca` on the system PATH for
# every user. Resources stay inside the app's own directory, so nothing is
# installed system-wide and nothing shadows a user's own `alpaca`.
#
# The output name has no target triple because it is resolved at runtime from
# the app's resource directory, not matched against a build triple.

set -eu

VERSION="0.0.15"
REPO="alpacahq/cli"
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
OUT="$ROOT/binaries"
DEST="$OUT/alpaca"

# target-triple -> release asset suffix
triple_for() {
  case "$1" in
    x86_64-unknown-linux-gnu)   echo "linux_amd64" ;;
    aarch64-unknown-linux-gnu)  echo "linux_arm64" ;;
    *) return 1 ;;
  esac
}

detect_triple() {
  case "$(uname -s)/$(uname -m)" in
    Linux/x86_64)               echo "x86_64-unknown-linux-gnu" ;;
    Linux/aarch64|Linux/arm64)  echo "aarch64-unknown-linux-gnu" ;;
    *) echo "unsupported platform: $(uname -s)/$(uname -m)" >&2; exit 1 ;;
  esac
}

fetch() {
  triple="$1"
  asset=$(triple_for "$triple") || {
    echo "error: no Alpaca CLI release for $triple" >&2
    exit 1
  }

  if [ -f "$DEST" ]; then
    echo "ok    binaries/alpaca (already present)"
    return 0
  fi

  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT

  release="https://github.com/$REPO/releases/download/v$VERSION"
  echo "get   cli_${VERSION}_${asset}"
  curl -fsSL -m 120 -o "$tmp/cli.tar.gz" \
    "$release/cli_${VERSION}_${asset}.tar.gz"
  curl -fsSL -m 60 -o "$tmp/checksums.txt" "$release/checksums.txt"

  # Verify before extracting; a corrupted download must never reach the bundle.
  expected=$(grep " cli_${VERSION}_${asset}.tar.gz$" "$tmp/checksums.txt" | awk '{print $1}')
  if [ -z "$expected" ]; then
    echo "error: no checksum listed for cli_${VERSION}_${asset}.tar.gz" >&2
    exit 1
  fi
  actual=$(sha256sum "$tmp/cli.tar.gz" | awk '{print $1}')
  if [ "$expected" != "$actual" ]; then
    echo "error: checksum mismatch for $asset" >&2
    echo "  expected $expected" >&2
    echo "  actual   $actual" >&2
    exit 1
  fi
  echo "ok    checksum verified"

  tar xzf "$tmp/cli.tar.gz" -C "$tmp" alpaca
  mkdir -p "$OUT"
  install -m 755 "$tmp/alpaca" "$DEST"
  echo "wrote $DEST"
}

fetch "$(detect_triple)"

echo
echo "binaries/ now holds: $(ls -1 "$OUT" | tr '\n' ' ')"
