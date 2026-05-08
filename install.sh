#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
#  ENI World Builder — One-Line Installer
#
#  Usage:
#    curl -fsSL https://raw.githubusercontent.com/joogleibooglei-web/AgentSilly/main/install.sh | bash
#
# ──────────────────────────────────────────────────────────────────────────────

set -euo pipefail

# ─── Colors & Formatting ─────────────────────────────────────────────────────

BOLD='\033[1m'
DIM='\033[2m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
PURPLE='\033[0;35m'
RESET='\033[0m'

print_banner() {
    echo ""
    echo -e "${PURPLE}${BOLD}"
    echo "  ╔══════════════════════════════════════════════╗"
    echo "  ║                                              ║"
    echo "  ║       🌍  ENI World Builder  v0.2.0          ║"
    echo "  ║                                              ║"
    echo "  ║   AI-powered world building for SillyTavern  ║"
    echo "  ║                                              ║"
    echo "  ╚══════════════════════════════════════════════╝"
    echo -e "${RESET}"
}

info()    { echo -e "  ${BLUE}▸${RESET} $1"; }
success() { echo -e "  ${GREEN}✓${RESET} $1"; }
warn()    { echo -e "  ${YELLOW}⚠${RESET} $1"; }
error()   { echo -e "  ${RED}✗${RESET} $1"; }
step()    { echo -e "\n${BOLD}  $1${RESET}"; }

# ─── Dependency Checks ───────────────────────────────────────────────────────

check_deps() {
    local missing=()

    if ! command -v git &>/dev/null; then
        missing+=("git")
    fi

    if ! command -v node &>/dev/null; then
        missing+=("node")
    fi

    if [ ${#missing[@]} -gt 0 ]; then
        error "Missing required dependencies: ${missing[*]}"
        echo ""
        echo "  Please install them and try again."
        exit 1
    fi

    # Check Node.js version (need 18+)
    local node_version
    node_version=$(node -v | sed 's/v//' | cut -d. -f1)
    if [ "$node_version" -lt 18 ]; then
        error "Node.js 18+ required (found v$(node -v))"
        exit 1
    fi
}

# ─── Platform Detection ──────────────────────────────────────────────────────

detect_platform() {
    local os arch

    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Darwin) os="macOS" ;;
        Linux)  os="Linux" ;;
        MINGW*|MSYS*|CYGWIN*) os="Windows" ;;
        *)
            error "Unsupported operating system: $os"
            exit 1
            ;;
    esac

    case "$arch" in
        x86_64|amd64) arch="x64" ;;
        arm64|aarch64) arch="arm64" ;;
        *)
            error "Unsupported architecture: $arch"
            exit 1
            ;;
    esac

    # Validate supported combinations
    if [[ "$os" == "Linux" && "$arch" == "arm64" ]]; then
        error "Linux ARM64 is not currently supported"
        exit 1
    fi

    info "Detected platform: ${BOLD}$os $arch${RESET}"
}

# ─── Get SillyTavern Path ───────────────────────────────────────────────────

get_st_path() {
    step "Where is your SillyTavern installation?"
    echo ""
    echo -e "  ${DIM}Enter the full path to your SillyTavern directory${RESET}"
    echo -e "  ${DIM}(e.g., /home/user/SillyTavern or C:\\Users\\user\\SillyTavern)${RESET}"
    echo ""
    printf "  ${BOLD}Path:${RESET} "
    read -r ST_PATH

    # Expand ~ if present
    ST_PATH="${ST_PATH/#\~/$HOME}"

    # Remove trailing slash
    ST_PATH="${ST_PATH%/}"

    # Validate the path
    if [ ! -d "$ST_PATH" ]; then
        error "Directory not found: $ST_PATH"
        exit 1
    fi

    # Check if it looks like a SillyTavern installation
    if [ ! -f "$ST_PATH/server.js" ] && [ ! -f "$ST_PATH/start.sh" ] && [ ! -f "$ST_PATH/package.json" ]; then
        warn "This doesn't look like a SillyTavern directory (no server.js or start.sh found)"
        printf "  Continue anyway? [y/N] "
        read -r confirm
        if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
            echo ""
            info "Installation cancelled."
            exit 0
        fi
    fi

    PLUGINS_DIR="$ST_PATH/plugins"
    PLUGIN_DIR="$PLUGINS_DIR/eni-world-builder"

    success "SillyTavern found at: ${BOLD}$ST_PATH${RESET}"
}

# ─── Enable Server Plugins ───────────────────────────────────────────────────

enable_plugins() {
    local config_file="$ST_PATH/config.yaml"

    if [ -f "$config_file" ]; then
        if grep -q "enableServerPlugins:" "$config_file"; then
            if grep -q "enableServerPlugins: true" "$config_file"; then
                success "Server plugins already enabled"
            else
                # Replace the line
                if [[ "$(uname -s)" == "Darwin" ]]; then
                    sed -i '' 's/enableServerPlugins:.*/enableServerPlugins: true/' "$config_file"
                else
                    sed -i 's/enableServerPlugins:.*/enableServerPlugins: true/' "$config_file"
                fi
                success "Enabled server plugins in config.yaml"
            fi
        else
            # Append the setting
            echo "enableServerPlugins: true" >> "$config_file"
            success "Added enableServerPlugins: true to config.yaml"
        fi
    else
        warn "config.yaml not found — you may need to enable server plugins manually"
        echo -e "  ${DIM}Add this to your config.yaml:${RESET}"
        echo -e "  ${DIM}  enableServerPlugins: true${RESET}"
    fi
}

# ─── Install Plugin ─────────────────────────────────────────────────────────

install_plugin() {
    # Create plugins directory if it doesn't exist
    if [ ! -d "$PLUGINS_DIR" ]; then
        mkdir -p "$PLUGINS_DIR"
        info "Created plugins directory"
    fi

    # Check if already installed
    if [ -d "$PLUGIN_DIR" ]; then
        warn "ENI World Builder is already installed at $PLUGIN_DIR"
        printf "  Reinstall (pull latest)? [Y/n] "
        read -r confirm
        if [[ "$confirm" =~ ^[Nn]$ ]]; then
            echo ""
            info "Installation cancelled."
            exit 0
        fi

        # Pull latest
        info "Updating existing installation..."
        cd "$PLUGIN_DIR"
        git pull origin main --quiet
        success "Updated to latest version"
    else
        # Fresh clone
        info "Cloning ENI World Builder..."
        git clone --quiet https://github.com/joogleibooglei-web/AgentSilly.git "$PLUGIN_DIR"
        success "Plugin cloned to ${BOLD}$PLUGIN_DIR${RESET}"
    fi
}

# ─── Verify Installation ────────────────────────────────────────────────────

verify_install() {
    if [ -f "$PLUGIN_DIR/index.js" ] && [ -f "$PLUGIN_DIR/plugin.json" ]; then
        success "Plugin files verified"
    else
        error "Installation appears incomplete — missing index.js or plugin.json"
        exit 1
    fi

    # Check plugin version
    local version
    version=$(node -e "console.log(require('$PLUGIN_DIR/plugin.json').version)" 2>/dev/null || echo "unknown")
    info "Installed version: ${BOLD}v$version${RESET}"
}

# ─── Done ────────────────────────────────────────────────────────────────────

print_success() {
    echo ""
    echo -e "${GREEN}${BOLD}"
    echo "  ┌──────────────────────────────────────────────┐"
    echo "  │                                              │"
    echo "  │   ✓  Installation complete!                  │"
    echo "  │                                              │"
    echo "  └──────────────────────────────────────────────┘"
    echo -e "${RESET}"
    echo -e "  ${BOLD}Next steps:${RESET}"
    echo ""
    echo -e "  1. Restart SillyTavern"
    echo -e "  2. The sidecar binary will download automatically on first launch"
    echo -e "  3. Open SillyTavern in your browser — the World Builder panel"
    echo -e "     will appear in the extensions sidebar"
    echo ""
    echo -e "  ${DIM}Trouble? Check the console for [ENI] log messages.${RESET}"
    echo -e "  ${DIM}Docs: https://github.com/joogleibooglei-web/AgentSilly${RESET}"
    echo ""
}

# ─── Main ────────────────────────────────────────────────────────────────────

main() {
    print_banner

    step "Checking dependencies..."
    check_deps
    detect_platform

    get_st_path

    step "Installing ENI World Builder..."
    enable_plugins
    install_plugin

    step "Verifying installation..."
    verify_install

    print_success
}

main
