#!/bin/sh
# traiding installer
#
#   curl -fsSL https://raw.githubusercontent.com/axiestudio/traiding/main/install.sh | sh
#
# Installs a prebuilt AppImage (or the distro package with --deb/--rpm), or
# builds from source with --from-source. Everything lands in user scope by
# default: no sudo, no system directories.

set -eu

APP="traiding"
REPO="${TRAIDING_REPO:-axiestudio/traiding}"
PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
DATA_DIR="$PREFIX/share"
APP_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/128x128/apps"
APP_DATA="${XDG_DATA_HOME:-$HOME/.local/share}/dev.traiding.terminal"
TMP_DIR=""

# ---------------------------------------------------------------------------
# output helpers
# ---------------------------------------------------------------------------

if [ -t 1 ]; then
  BOLD=$(printf '\033[1m'); DIM=$(printf '\033[2m')
  RED=$(printf '\033[31m'); GREEN=$(printf '\033[32m'); YELLOW=$(printf '\033[33m')
  RESET=$(printf '\033[0m')
else
  BOLD=''; DIM=''; RED=''; GREEN=''; YELLOW=''; RESET=''
fi

say()  { printf '%s\n' "$*"; }
step() { printf '%s==>%s %s\n' "$BOLD" "$RESET" "$*"; }
ok()   { printf '%s  ok%s %s\n' "$GREEN" "$RESET" "$*"; }
warn() { printf '%swarn%s %s\n' "$YELLOW" "$RESET" "$*" >&2; }
die()  { printf '%serror%s %s\n' "$RED" "$RESET" "$*" >&2; exit 1; }

cleanup() {
  if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT INT TERM

# ---------------------------------------------------------------------------
# arguments
# ---------------------------------------------------------------------------

MODE="appimage"
ACTION="install"

usage() {
  cat <<EOF
${BOLD}traiding installer${RESET}

Usage: install.sh [options]

Options:
  --from-source    Build with cargo instead of downloading a release
  --appimage       Install the AppImage (default)
  --deb            Install the .deb package (needs sudo)
  --rpm            Install the .rpm package (needs sudo)
  --prefix DIR     Install prefix for user-scope installs (default: ~/.local)
  --uninstall      Remove traiding from the prefix
  -h, --help       Show this message

Environment:
  TRAIDING_REPO    GitHub repo to download from (default: $REPO)
  TRAIDING_VERSION Pin a release tag, e.g. v0.1.0
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --from-source) MODE="source" ;;
    --appimage)    MODE="appimage" ;;
    --deb)         MODE="deb" ;;
    --rpm)         MODE="rpm" ;;
    --uninstall)   ACTION="uninstall" ;;
    --prefix)      shift; [ $# -gt 0 ] || die "--prefix needs a directory"; PREFIX="$1"
                   BIN_DIR="$PREFIX/bin"; DATA_DIR="$PREFIX/share"
                   APP_DIR="$DATA_DIR/applications"; ICON_DIR="$DATA_DIR/icons/hicolor/128x128/apps" ;;
    -h|--help)     usage; exit 0 ;;
    *)             die "unknown option: $1 (try --help)" ;;
  esac
  shift
done

# ---------------------------------------------------------------------------
# uninstall
# ---------------------------------------------------------------------------

if [ "$ACTION" = "uninstall" ]; then
  step "Removing $APP from $PREFIX"
  rm -f "$BIN_DIR/$APP"
  rm -f "$APP_DIR/$APP.desktop"
  rm -f "$ICON_DIR/$APP.png"
  rm -f "$DATA_DIR/$APP/$APP.AppImage"
  rmdir "$DATA_DIR/$APP" 2>/dev/null || true
  ok "removed"
  say ""
  say "${DIM}Credentials were never written to disk. Local history remains at:${RESET}"
  say "${DIM}  $APP_DATA/storage/traiding.db${RESET}"
  say "${DIM}Delete that folder to reset preferences and history.${RESET}"
  exit 0
fi

# ---------------------------------------------------------------------------
# environment checks
# ---------------------------------------------------------------------------

need() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

detect_arch() {
  machine=$(uname -m)
  case "$machine" in
    x86_64|amd64)  echo "x86_64" ;;
    aarch64|arm64) echo "aarch64" ;;
    *)             die "unsupported architecture: $machine" ;;
  esac
}

check_webkit() {
  # Tauri links against webkit2gtk 4.1 on Linux. Distro package names differ,
  # so just probe for the shared library and print the right hint.
  if ldconfig -p 2>/dev/null | grep -q 'libwebkit2gtk-4\.1\.so'; then
    return 0
  fi

  if [ -f /etc/os-release ]; then
    . /etc/os-release
    case "${ID:-}${ID_LIKE:-}" in
      *debian*|*ubuntu*)
        die "missing webkit2gtk 4.1. Install it with:
     sudo apt install libwebkit2gtk-4.1-0" ;;
      *fedora*|*rhel*)
        die "missing webkit2gtk 4.1. Install it with:
     sudo dnf install webkit2gtk4.1" ;;
      *arch*)
        die "missing webkit2gtk 4.1. Install it with:
     sudo pacman -S webkit2gtk-4.1" ;;
    esac
  fi

  die "missing webkit2gtk 4.1 — install your distro's webkit2gtk-4.1 package"
}

install_desktop_entry() {
  mkdir -p "$APP_DIR" "$ICON_DIR"

  if [ -f "$ICON_SRC" ]; then
    cp "$ICON_SRC" "$ICON_DIR/$APP.png"
  fi

  cat > "$APP_DIR/$APP.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=traiding
GenericName=Trading Terminal
Comment=Alpaca paper and live trading terminal
Exec=$BIN_DIR/$APP %U
Icon=$APP
Terminal=false
Categories=Office;Finance;
Keywords=trading;stocks;alpaca;tradingview;portfolio;
StartupWMClass=traiding
EOF

  chmod 644 "$APP_DIR/$APP.desktop"
  command -v update-desktop-database >/dev/null 2>&1 &&
    update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
}

warn_path() {
  case ":${PATH}:" in
    *":$BIN_DIR:"*) ;;
    *)
      say ""
      warn "$BIN_DIR is not on your PATH. Add this to your shell profile:"
      say ""
      say "    export PATH=\"$BIN_DIR:\$PATH\""
      say ""
      ;;
  esac
}

# ---------------------------------------------------------------------------
# source build
# ---------------------------------------------------------------------------

build_from_source() {
  step "Building from source"

  need cargo
  need npm

  if [ ! -f "Cargo.toml" ] || [ ! -d "ui" ]; then
    die "--from-source must run from the repo root"
  fi

  say "${DIM}This compiles the UI and then the Rust binary with LTO; expect a few minutes.${RESET}"
  npm install --no-fund --no-audit
  npm --prefix ui install --no-fund --no-audit
  npm run build -- --no-bundle

  BINARY="target/release/$APP"
  [ -x "$BINARY" ] || die "build finished but $BINARY is missing"

  mkdir -p "$BIN_DIR"
  install -m 755 "$BINARY" "$BIN_DIR/$APP"
  ICON_SRC="icons/128x128.png"
}

# ---------------------------------------------------------------------------
# release download
# ---------------------------------------------------------------------------

latest_tag() {
  curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null |
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
    head -1
}

download_appimage() {
  step "Downloading $APP"

  need curl
  arch=$(detect_arch)
  case "$arch" in
    x86_64) appimage_arch="amd64" ;;
    *)      appimage_arch="$arch" ;;
  esac

  tag="${TRAIDING_VERSION:-$(latest_tag || true)}"
  if [ -z "${tag:-}" ]; then
    die "could not determine the latest release of $REPO.
     Set TRAIDING_VERSION=v0.1.0, or use --from-source."
  fi
  say "${DIM}release $tag, architecture $appimage_arch${RESET}"

  TMP_DIR=$(mktemp -d)
  url="https://github.com/$REPO/releases/download/$tag/${APP}_${tag#v}_${appimage_arch}.AppImage"

  if ! curl -fL --progress-bar "$url" -o "$TMP_DIR/$APP.AppImage"; then
    die "download failed: $url
     Check that the release publishes an AppImage for $arch, or use --from-source."
  fi

  mkdir -p "$DATA_DIR/$APP" "$BIN_DIR"
  install -m 755 "$TMP_DIR/$APP.AppImage" "$DATA_DIR/$APP/$APP.AppImage"

  # The AppImage itself is the executable; a symlink keeps `traiding` on PATH
  # without copying a ~20 MB file into bin.
  ln -sf "$DATA_DIR/$APP/$APP.AppImage" "$BIN_DIR/$APP"

  ICON_SRC=""
  curl -fsSL "https://raw.githubusercontent.com/$REPO/$tag/icons/128x128.png" \
    -o "$TMP_DIR/icon.png" 2>/dev/null && ICON_SRC="$TMP_DIR/icon.png" || true
}

download_deb() {
  step "Downloading the .deb package"

  need curl
  tag="${TRAIDING_VERSION:-$(latest_tag || true)}"
  [ -n "${tag:-}" ] || die "could not determine the latest release; set TRAIDING_VERSION"

  arch=$(detect_arch)
  TMP_DIR=$(mktemp -d)

  # Tauri names amd64 packages; map from the uname value.
  case "$arch" in
    x86_64) deb_arch="amd64" ;;
    aarch64) deb_arch="arm64" ;;
  esac

  url="https://github.com/$REPO/releases/download/$tag/${APP}_${tag#v}_${deb_arch}.deb"
  curl -fL --progress-bar "$url" -o "$TMP_DIR/$APP.deb" || die "download failed: $url"

  step "Installing the package (sudo required)"
  sudo dpkg -i "$TMP_DIR/$APP.deb" || sudo apt-get install -f -y
  ok "installed system-wide"
  exit 0
}

download_rpm() {
  step "Downloading the .rpm package"

  need curl
  tag="${TRAIDING_VERSION:-$(latest_tag || true)}"
  [ -n "${tag:-}" ] || die "could not determine the latest release; set TRAIDING_VERSION"

  arch=$(detect_arch)
  TMP_DIR=$(mktemp -d)
  url="https://github.com/$REPO/releases/download/$tag/${APP}-${tag#v}-1.${arch}.rpm"

  curl -fL --progress-bar "$url" -o "$TMP_DIR/$APP.rpm" || die "download failed: $url"

  step "Installing the package (sudo required)"
  sudo rpm -Uvh --force "$TMP_DIR/$APP.rpm"
  ok "installed system-wide"
  exit 0
}

# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

check_webkit

case "$MODE" in
  source)   build_from_source ;;
  appimage) download_appimage ;;
  deb)      download_deb ;;
  rpm)      download_rpm ;;
esac

install_desktop_entry

say ""
ok "$APP installed to $BIN_DIR/$APP"
say ""
say "  Run it:   ${BOLD}$APP${RESET}"
say "  Local:    $APP_DATA/storage/traiding.db"
say "  Remove:   curl -fsSL https://raw.githubusercontent.com/$REPO/main/install.sh | sh -s -- --uninstall --prefix $PREFIX"
say ""
say "${DIM}Credentials stay in memory and are never written to disk.${RESET}"
say "${DIM}Preferences and market history stay in the app-local database.${RESET}"

warn_path
