#!/bin/sh
#
# synx installation script — universal Linux / macOS installer.
# Usage:
#     curl -fsSL https://raw.githubusercontent.com/Muvon/synx/main/install.sh | sh
#     curl -fsSL https://raw.githubusercontent.com/Muvon/synx/main/install.sh | sh -s -- --version 0.1.0
#

set -eu

REPO="Muvon/synx"
BINARY_NAME="synx"
INSTALL_DIR="${SYNX_INSTALL_DIR:-$HOME/.local/bin}"

# ── colors (best-effort; many CI shells handle these) ──────────────────
if [ -t 1 ] && command -v tput >/dev/null 2>&1; then
    BOLD=$(tput bold || true)
    RED=$(tput setaf 1 || true)
    GREEN=$(tput setaf 2 || true)
    YELLOW=$(tput setaf 3 || true)
    BLUE=$(tput setaf 4 || true)
    RESET=$(tput sgr0 || true)
else
    BOLD=""; RED=""; GREEN=""; YELLOW=""; BLUE=""; RESET=""
fi

info()    { printf "%s•%s %s\n"  "$BLUE"   "$RESET" "$*"; }
ok()      { printf "%s✓%s %s\n"  "$GREEN"  "$RESET" "$*"; }
warn()    { printf "%s!%s %s\n"  "$YELLOW" "$RESET" "$*" >&2; }
fail()    { printf "%s✗%s %s\n"  "$RED"    "$RESET" "$*" >&2; exit 1; }

command_exists() { command -v "$1" >/dev/null 2>&1; }

# ── detect platform → release target triple ───────────────────────────
detect_target() {
    os=""
    arch=""
    case "$(uname -s)" in
        Linux*)  os="unknown-linux-musl" ;;
        Darwin*) os="apple-darwin" ;;
        *)       fail "Unsupported OS: $(uname -s) — synx is Linux/macOS only" ;;
    esac
    case "$(uname -m)" in
        x86_64|amd64)   arch="x86_64" ;;
        arm64|aarch64)  arch="aarch64" ;;
        *)              fail "Unsupported architecture: $(uname -m)" ;;
    esac
    printf "%s-%s\n" "$arch" "$os"
}

# ── fetch latest version tag ──────────────────────────────────────────
get_latest_version() {
    auth=""
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        auth="-H Authorization: token $GITHUB_TOKEN"
    elif [ -n "${GH_TOKEN:-}" ]; then
        auth="-H Authorization: token $GH_TOKEN"
    fi
    v=$(curl -fsSL $auth "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
        | grep '"tag_name":' \
        | head -1 \
        | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$v" ]; then
        v=$(curl -fsSL $auth "https://api.github.com/repos/$REPO/releases" 2>/dev/null \
            | grep '"tag_name":' \
            | head -1 \
            | sed -E 's/.*"([^"]+)".*/\1/')
    fi
    if [ -z "$v" ]; then
        fail "Could not determine latest version. Check https://github.com/$REPO/releases"
    fi
    printf "%s\n" "$v"
}

check_prereqs() {
    command_exists curl || fail "curl is required (apt install curl / brew install curl)"
    command_exists tar  || fail "tar is required"
}

download_and_install() {
    version="$1"
    target="$2"

    tmp=$(mktemp -d 2>/dev/null || mktemp -d -t synx-install)
    trap 'rm -rf "$tmp"' EXIT INT TERM

    archive="synx-${version}-${target}.tar.gz"
    url="https://github.com/${REPO}/releases/download/${version}/${archive}"

    info "Downloading ${BOLD}${archive}${RESET}"
    info "  from $url"
    if ! curl -fsSL "$url" -o "$tmp/$archive"; then
        fail "Download failed. Verify the release exists: $url"
    fi

    info "Extracting…"
    (cd "$tmp" && tar xzf "$archive") || fail "Extraction failed"

    binary="$tmp/$BINARY_NAME"
    [ -f "$binary" ] || fail "$BINARY_NAME not found in archive"

    mkdir -p "$INSTALL_DIR"
    info "Installing to ${BOLD}${INSTALL_DIR}${RESET}"
    cp "$binary" "$INSTALL_DIR/$BINARY_NAME"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
    ok "$BINARY_NAME installed"
}

check_path_warning() {
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            warn "$INSTALL_DIR is not on your \$PATH"
            printf "  Add to your shell rc (e.g. ~/.bashrc, ~/.zshrc):\n"
            printf "    %sexport PATH=\"%s:\$PATH\"%s\n" "$BOLD" "$INSTALL_DIR" "$RESET"
            ;;
    esac
}

verify() {
    if "$INSTALL_DIR/$BINARY_NAME" --version >/dev/null 2>&1; then
        v=$("$INSTALL_DIR/$BINARY_NAME" --version 2>/dev/null)
        ok "$v"
    else
        fail "Installed binary failed to run"
    fi
}

usage() {
    cat <<EOF
synx installer

USAGE:
    install.sh [OPTIONS]

OPTIONS:
    --version <VERSION>       Install a specific version (default: latest)
    --target <TRIPLE>         Override auto-detected target triple
    --install-dir <DIR>       Install destination (default: \$HOME/.local/bin
                              or \$SYNX_INSTALL_DIR)
    -h, --help                Show this help

ENVIRONMENT:
    SYNX_INSTALL_DIR          Same as --install-dir
    SYNX_VERSION              Same as --version
    GITHUB_TOKEN / GH_TOKEN   Avoid GitHub API rate limits

SUPPORTED TARGETS:
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-musl
    x86_64-apple-darwin
    aarch64-apple-darwin

EXAMPLES:
    curl -fsSL https://raw.githubusercontent.com/Muvon/synx/main/install.sh | sh
    curl -fsSL https://raw.githubusercontent.com/Muvon/synx/main/install.sh | sh -s -- --version 0.1.0
    SYNX_INSTALL_DIR=/usr/local/bin curl -fsSL .../install.sh | sh
EOF
}

main() {
    version=""
    target=""

    while [ $# -gt 0 ]; do
        case "$1" in
            --version)      version="$2"; shift 2 ;;
            --target)       target="$2";  shift 2 ;;
            --install-dir)  INSTALL_DIR="$2"; shift 2 ;;
            -h|--help)      usage; exit 0 ;;
            *)              warn "Unknown option: $1"; usage; exit 1 ;;
        esac
    done

    version="${version:-${SYNX_VERSION:-}}"

    check_prereqs
    [ -n "$target" ]  || target=$(detect_target)
    info "Target: $target"
    [ -n "$version" ] || { info "Resolving latest version…"; version=$(get_latest_version); }
    info "Version: $version"

    download_and_install "$version" "$target"
    check_path_warning
    verify

    printf "\n"
    ok "Done! Run ${BOLD}synx --help${RESET} to get started."
}

main "$@"
