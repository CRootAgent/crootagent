#!/usr/bin/env bash
set -eo pipefail

# Lontar CLI Interactive Installer for macOS & Linux
# Minimalist AI Agent with NotebookLM Grounding & OpenCode Terminal UX

REPO="CRootAgent/crootagent"
DEFAULT_INSTALL_DIR="$HOME/.local/bin"
TARGET_DIR="${LONTAR_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
AUTO_YES=false

# Parse command line flags
while [ "$#" -gt 0 ]; do
    case "$1" in
        -y|--yes)
            AUTO_YES=true
            shift
            ;;
        --dir)
            TARGET_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: install.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  -y, --yes        Non-interactive mode (accept default choices)"
            echo "  --dir <PATH>     Custom install directory (default: ~/.local/bin)"
            echo "  -h, --help       Show this help message"
            exit 0
            ;;
        *)
            shift
            ;;
    esac
done

# ANSI Styling Definitions
BOLD="\033[1m"
DIM="\033[2m"
WHITE="\033[97m"
CYAN="\033[36m"
GREEN="\033[32m"
YELLOW="\033[33m"
GRAY="\033[90m"
RESET="\033[0m"

# 4-Color Rainbow Block (Cyan, Blue, Coral Pink, Warm Yellow)
C_CYAN="\033[38;2;86;204;242m"
C_BLUE="\033[38;2;47;128;237m"
C_PINK="\033[38;2;255;110;110m"
C_YELLOW="\033[38;2;242;201;76m"

# Disable colors if terminal doesn't support them or NO_COLOR is set
if [ -n "${NO_COLOR:-}" ] || [ "${TERM:-}" = "dumb" ]; then
    BOLD=""
    DIM=""
    WHITE=""
    CYAN=""
    GREEN=""
    YELLOW=""
    GRAY=""
    RESET=""
    C_CYAN=""
    C_BLUE=""
    C_PINK=""
    C_YELLOW=""
fi

TARGET_BIN="${TARGET_DIR}/lontar"

# Helper to render progress bar
render_bar() {
    local pct="$1"
    local msg="$2"
    local frame="$3"
    local total_slots=20
    local max_pos=$((total_slots - 4))
    local pos=$(( (pct * max_pos) / 100 ))
    if [ "$pos" -lt 0 ]; then pos=0; fi
    if [ "$pos" -gt "$max_pos" ]; then pos=$max_pos; fi

    local left_dots=""
    for ((i=0; i<pos; i++)); do left_dots="${left_dots}░"; done
    local right_dots=""
    for ((i=pos+4; i<total_slots; i++)); do right_dots="${right_dots}░"; done

    local rainbow="${C_CYAN}█${C_BLUE}█${C_PINK}█${C_YELLOW}█${RESET}"
    printf "\r  ${CYAN}%s${RESET} ${GRAY}%s${RESET}%s${GRAY}%s${RESET} %s" "$frame" "$left_dots" "$rainbow" "$right_dots" "$msg"
}

# 1. Show Installer Banner and Menu
echo ""
echo -e "${BOLD}${WHITE}  ██        ▄██▄${RESET}"
echo -e "${BOLD}${WHITE}  ██          ▀██▄${RESET}"
echo -e "${BOLD}${WHITE}  ██            ▀██▄${RESET}"
echo -e "${BOLD}${WHITE}  ██              ███${RESET}"
echo -e "${BOLD}${WHITE}  ██            ▄██▀${RESET}"
echo -e "${BOLD}${WHITE}▄▄██████      ▄██▀    ▄█▄${RESET}"
echo -e "${BOLD}${WHITE}▀▀▀▀▀▀▀▀    ▀▀▀▀      ▀▀▀${RESET}"
echo ""
echo -e "${BOLD}${WHITE}Lontar Installer${RESET}"
echo -e "${DIM}There are many agent harnesses but this one is yours${RESET}"
echo ""
echo -e "${BOLD}${WHITE}Installation:${RESET}"
echo ""
echo -e "  Lontar will install to ${WHITE}${TARGET_BIN}${RESET}"
echo ""
echo -e "${BOLD}${WHITE}Choose an action:${RESET}"
echo ""
echo -e "  ${CYAN}y${RESET}    ${GREEN}Install Lontar${RESET} ${DIM}(default)${RESET}"
echo -e "  ${CYAN}n${RESET}    ${DIM}Do nothing${RESET}"
echo ""

# 2. Handle User Choice (supporting piped stdin from curl | bash via /dev/tty)
CHOICE="y"
if [ "$AUTO_YES" = false ]; then
    if [ -t 0 ]; then
        read -r -n 1 -s input_choice || input_choice="y"
    elif [ -e /dev/tty ]; then
        read -r -n 1 -s input_choice < /dev/tty || input_choice="y"
    else
        input_choice="y"
    fi

    if [ -n "$input_choice" ]; then
        CHOICE="$input_choice"
    fi
fi

case "$CHOICE" in
    y|Y|"")
        echo -e "Will install Lontar."
        echo ""
        ;;
    n|N|q|Q)
        echo -e "${DIM}Installation cancelled.${RESET}"
        exit 0
        ;;
    *)
        echo -e "${DIM}Action not recognized, aborting.${RESET}"
        exit 1
        ;;
esac

# 3. Detect Platform and Architecture
OS="$(uname -s)"
case "$OS" in
    Darwin)
        PLATFORM="macos"
        ;;
    Linux)
        PLATFORM="linux"
        ;;
    *)
        echo "❌ Operating system $OS is not supported by this installer."
        exit 1
        ;;
esac

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)
        ARCH_SUFFIX="x86_64"
        ;;
    arm64|aarch64)
        if [ "$PLATFORM" = "macos" ]; then
            ARCH_SUFFIX="arm64"
        else
            ARCH_SUFFIX="aarch64"
        fi
        ;;
    *)
        echo "❌ Machine architecture $ARCH is not supported."
        exit 1
        ;;
esac

ASSET_NAME="lontar-${PLATFORM}-${ARCH_SUFFIX}.tar.gz"

# 4. Animated Installation Lifecycle
SPIN_CHARS=("⠋" "⠙" "⠹" "⠸" "⠼" "⠴" "⠦" "⠧" "⠇" "⠏")
FRAME_IDX=0

next_frame() {
    local f="${SPIN_CHARS[$FRAME_IDX]}"
    FRAME_IDX=$(( (FRAME_IDX + 1) % 10 ))
    echo "$f"
}

TMP_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

# Phase A: Target Preparation
render_bar 15 "Preparing installation directory (15%)" "$(next_frame)"
mkdir -p "$TARGET_DIR"
sleep 0.15

# Phase B: Source Acquisition (Local build, Online Release, or Cargo fallback)
render_bar 32 "Checking system architecture ${PLATFORM}-${ARCH_SUFFIX} (32%)" "$(next_frame)"
sleep 0.15

render_bar 44 "Installing Lontar fetching tarballs (44%)" "$(next_frame)"
sleep 0.2

INSTALLED_FROM=""

# Priority 1: Local release build in repository
if [ -f "crootagent/target/release/lontar" ]; then
    cp "crootagent/target/release/lontar" "$TMP_DIR/lontar"
    INSTALLED_FROM="local"
elif [ -f "target/release/lontar" ]; then
    cp "target/release/lontar" "$TMP_DIR/lontar"
    INSTALLED_FROM="local"
elif [ -f "../target/release/lontar" ]; then
    cp "../target/release/lontar" "$TMP_DIR/lontar"
    INSTALLED_FROM="local"
else
    # Priority 2: Online release tarball
    LATEST_URL="https://github.com/${REPO}/releases/latest/download/${ASSET_NAME}"
    render_bar 58 "Downloading release archive from GitHub (58%)" "$(next_frame)"
    if curl -sSL -f "$LATEST_URL" -o "$TMP_DIR/$ASSET_NAME" 2>/dev/null; then
        tar -xzf "$TMP_DIR/$ASSET_NAME" -C "$TMP_DIR" 2>/dev/null || true
        if [ -f "$TMP_DIR/lontar" ]; then
            INSTALLED_FROM="online"
        fi
    fi

    # Priority 3: Fallback compilation via Cargo if in source tree
    if [ -z "$INSTALLED_FROM" ]; then
        if [ -f "Cargo.toml" ] || [ -f "crootagent/Cargo.toml" ]; then
            render_bar 65 "Compiling Lontar from source with Cargo (65%)" "$(next_frame)"
            if [ -f "crootagent/Cargo.toml" ]; then
                (cd crootagent && cargo build --release -q)
                cp "crootagent/target/release/lontar" "$TMP_DIR/lontar"
            else
                cargo build --release -q
                cp "target/release/lontar" "$TMP_DIR/lontar"
            fi
            INSTALLED_FROM="cargo"
        fi
    fi
fi

# Fallback check if existing binary exists on user system
if [ ! -f "$TMP_DIR/lontar" ] && [ -f "$HOME/.local/bin/lontar" ]; then
    cp "$HOME/.local/bin/lontar" "$TMP_DIR/lontar"
    INSTALLED_FROM="system"
fi

if [ ! -f "$TMP_DIR/lontar" ]; then
    printf "\r                                                                                    \r"
    echo -e "❌ Could not find or download Lontar binary for ${PLATFORM}-${ARCH_SUFFIX}."
    echo -e "   Please build locally using: ${CYAN}cd crootagent && cargo build --release${RESET}"
    exit 1
fi

render_bar 78 "Extracting binary to destination (78%)" "$(next_frame)"
cp "$TMP_DIR/lontar" "$TARGET_BIN"
sleep 0.15

render_bar 90 "Configuring execution permissions (90%)" "$(next_frame)"
chmod +x "$TARGET_BIN"
sleep 0.15

render_bar 100 "Finalizing Lontar installation (100%)" "$(next_frame)"
sleep 0.2

# Clear line and print final success status
printf "\r                                                                                    \r"
echo -e "  ${GREEN}✔${RESET} ${BOLD}Installed Lontar successfully to ${WHITE}${TARGET_BIN}${RESET}!"
echo ""

# 5. Check PATH & instructions
if [[ ":$PATH:" != *":$TARGET_DIR:"* ]]; then
    echo -e "${YELLOW}Notice:${RESET} ${TARGET_DIR} is not currently in your PATH."
    echo -e "Add it to your shell configuration:"
    echo -e "  ${DIM}echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc${RESET}"
    echo -e "  ${DIM}export PATH=\"\$HOME/.local/bin:\$PATH\"${RESET}"
    echo ""
fi

echo -e "${DIM}Run ${WHITE}'lontar'${RESET}${DIM} to launch the TUI directly (Zero-Config AI active).${RESET}"
echo ""
