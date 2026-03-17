#!/usr/bin/env sh
# Install script for `plan` — CLI task tracker for AI agents
# Usage: curl -fsSL https://raw.githubusercontent.com/Marlinski/plan/main/install.sh | sh

set -e

REPO="Marlinski/plan"
BINARY="plan"
INSTALL_DIR=""

# ── Detect OS and architecture ───────────────────────────────────────────────

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-unknown-linux-musl" ;;
      aarch64) TARGET="aarch64-unknown-linux-musl" ;;
      arm*)    TARGET="armv7-unknown-linux-musleabihf" ;;
      *)
        echo "Unsupported architecture: $ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-apple-darwin" ;;
      arm64)   TARGET="aarch64-apple-darwin" ;;
      *)
        echo "Unsupported architecture: $ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    TARGET="x86_64-pc-windows-msvc"
    BINARY="plan.exe"
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

# ── Determine install directory ───────────────────────────────────────────────

if [ "$(id -u)" = "0" ]; then
  INSTALL_DIR="/usr/local/bin"
else
  INSTALL_DIR="$HOME/.local/bin"
  mkdir -p "$INSTALL_DIR"
fi

# ── Fetch latest release tag ─────────────────────────────────────────────────

echo "Fetching latest release..."

if command -v curl > /dev/null 2>&1; then
  LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' \
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
elif command -v wget > /dev/null 2>&1; then
  LATEST=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' \
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
else
  echo "Error: curl or wget is required" >&2
  exit 1
fi

if [ -z "$LATEST" ]; then
  echo "Error: could not determine latest release" >&2
  exit 1
fi

echo "Latest release: $LATEST"

# ── Download binary ───────────────────────────────────────────────────────────

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST}/${BINARY}-${TARGET}"

if [ "$OS" = "Windows_NT" ] || echo "$OS" | grep -q "MINGW\|MSYS\|CYGWIN"; then
  DOWNLOAD_URL="${DOWNLOAD_URL}.exe"
fi

echo "Downloading $BINARY for $TARGET..."
TMP_FILE=$(mktemp)

if command -v curl > /dev/null 2>&1; then
  curl -fsSL "$DOWNLOAD_URL" -o "$TMP_FILE"
else
  wget -qO "$TMP_FILE" "$DOWNLOAD_URL"
fi

# ── Install ───────────────────────────────────────────────────────────────────

chmod +x "$TMP_FILE"
mv "$TMP_FILE" "$INSTALL_DIR/$BINARY"

# Copy SKILL.md to ~/.local/share/plan/SKILL.md for `plan skill` command
SKILL_DIR="$HOME/.local/share/plan"
mkdir -p "$SKILL_DIR"
SKILL_URL="https://raw.githubusercontent.com/${REPO}/main/SKILL.md"
if command -v curl > /dev/null 2>&1; then
  curl -fsSL "$SKILL_URL" -o "$SKILL_DIR/SKILL.md" 2>/dev/null || true
else
  wget -qO "$SKILL_DIR/SKILL.md" "$SKILL_URL" 2>/dev/null || true
fi

# ── Success ───────────────────────────────────────────────────────────────────

echo ""
echo "Installed: $INSTALL_DIR/$BINARY ($LATEST)"
echo ""

# Check if install dir is in PATH
case ":$PATH:" in
  *":$INSTALL_DIR:"*)
    ;;
  *)
    echo "NOTE: Add $INSTALL_DIR to your PATH:"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
    echo ""
    ;;
esac

echo "Quick start:"
echo "  cd /your/project"
echo "  plan init"
echo "  plan register"
echo "  plan skill          # read the agent onboarding guide"
echo ""
echo "For AI agents, read the skill file:"
echo "  plan skill"
