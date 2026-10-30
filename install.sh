#!/usr/bin/env bash
set -e

# RAC (Rust Android Compiler) Installer for Linux and macOS

REPO="sshremoteh/RAC"
BINARY_NAME="rac"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_err() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 1. Determine destination directory
if [ "$EUID" -eq 0 ]; then
    INSTALL_DIR="/usr/local/bin"
else
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
fi

TARGET_BIN="$INSTALL_DIR/$BINARY_NAME"

log_info "Installing RAC (Rust Android Compiler)..."
log_info "Destination: $TARGET_BIN"

# 2. Check if installer is running from local repo build
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || true)"
LOCAL_RELEASE_BIN="$SCRIPT_DIR/target/release/$BINARY_NAME"

installed=false

if [ -f "$LOCAL_RELEASE_BIN" ]; then
    log_info "Found local compiled binary at $LOCAL_RELEASE_BIN"
    cp "$LOCAL_RELEASE_BIN" "$TARGET_BIN"
    chmod +x "$TARGET_BIN"
    installed=true
fi

# 3. If not installed from local binary, attempt to download from GitHub release
if [ "$installed" = false ]; then
    OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
    ARCH="$(uname -m)"

    case "$ARCH" in
        x86_64|amd64)
            TARGET_ARCH="x86_64"
            ;;
        aarch64|arm64)
            TARGET_ARCH="aarch64"
            ;;
        *)
            log_warn "Unsupported architecture $ARCH for prebuilt binary. Will try building from source."
            TARGET_ARCH=""
            ;;
    esac

    if [ -n "$TARGET_ARCH" ]; then
        LATEST_URL="https://github.com/$REPO/releases/latest/download/rac-${TARGET_ARCH}-${OS}.tar.gz"
        log_info "Checking for prebuilt release: $LATEST_URL"

        TMP_DIR="$(mktemp -d)"
        trap 'rm -rf "$TMP_DIR"' EXIT

        if curl -fsSL "$LATEST_URL" -o "$TMP_DIR/rac.tar.gz" 2>/dev/null; then
            tar -xzf "$TMP_DIR/rac.tar.gz" -C "$TMP_DIR"
            cp "$TMP_DIR/$BINARY_NAME" "$TARGET_BIN"
            chmod +x "$TARGET_BIN"
            installed=true
            log_success "Downloaded prebuilt binary from GitHub release."
        fi
    fi
fi

# 4. If prebuilt binary is not yet available, fallback to cargo install
if [ "$installed" = false ]; then
    if command -v cargo >/dev/null 2>&1; then
        log_info "Prebuilt release binary not found. Building with Cargo..."
        cargo install --git "https://github.com/$REPO.git" rac-cli --bin rac --root "$HOME/.local"
        if [ "$INSTALL_DIR" != "$HOME/.local/bin" ] && [ -f "$HOME/.local/bin/$BINARY_NAME" ]; then
            cp "$HOME/.local/bin/$BINARY_NAME" "$TARGET_BIN"
        fi
        installed=true
    else
        log_err "Could not download prebuilt binary, and 'cargo' is not installed."
        log_err "Please install Rust (https://rustup.rs) or check GitHub releases at https://github.com/$REPO/releases"
        exit 1
    fi
fi

# 5. Check PATH environment
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    log_warn "$INSTALL_DIR is not currently in your PATH."
    SHELL_NAME="$(basename "$SHELL")"
    RC_FILE=""
    case "$SHELL_NAME" in
        bash) RC_FILE="$HOME/.bashrc" ;;
        zsh)  RC_FILE="$HOME/.zshrc" ;;
        fish) RC_FILE="$HOME/.config/fish/config.fish" ;;
    esac

    if [ -n "$RC_FILE" ] && [ -f "$RC_FILE" ]; then
        if ! grep -q "$INSTALL_DIR" "$RC_FILE"; then
            echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$RC_FILE"
            log_info "Added $INSTALL_DIR to $RC_FILE. Please restart your terminal or run: source $RC_FILE"
        fi
    else
        log_info "Add this to your shell profile: export PATH=\"$INSTALL_DIR:\$PATH\""
    fi
fi

# 6. Verify installation
if "$TARGET_BIN" --version >/dev/null 2>&1; then
    VER="$("$TARGET_BIN" --version)"
    log_success "RAC successfully installed: $VER"
    echo ""
    echo "Get started with:"
    echo "  rac new my_app"
    echo "  rac build my_app"
else
    log_err "Installation failed verification."
    exit 1
fi
