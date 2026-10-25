#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# Zephyr — Userspace MS-SSTP Client & Dual SOCKS5/HTTP Proxy
# Fast, lightweight, zero-root network proxy
# Supports: Linux, macOS, Android (Termux)
# ==============================================================================

readonly BIN_NAME="zephyr"
readonly REPO="${ZEPHYR_REPO:-hossein/zephyr}"
readonly API_BASE="https://api.github.com/repos/${REPO}"

# Terminal colors
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[0;33m'
readonly BLUE='\033[0;34m'
readonly CYAN='\033[0;36m'
readonly BOLD='\033[1m'
readonly RESET='\033[0m'

info()    { echo -e "${BLUE}[*]${RESET} $*"; }
success() { echo -e "${GREEN}[+]${RESET} $*"; }
warn()    { echo -e "${YELLOW}[!]${RESET} $*"; }
error()   { echo -e "${RED}[-]${RESET} $*" >&2; }

TMP_DIR=""
cleanup() {
  if [[ -n "${TMP_DIR:-}" && -d "${TMP_DIR:-}" ]]; then
    rm -rf "${TMP_DIR}"
  fi
  return 0
}
trap cleanup EXIT INT TERM

# Detect Environment (Termux vs standard Linux/macOS)
detect_environment() {
  if [[ -n "${TERMUX_VERSION:-}" ]] || [[ -d "/data/data/com.termux/files/usr" ]]; then
    echo "termux"
  elif [[ "$OSTYPE" == "darwin"* ]]; then
    echo "macos"
  else
    echo "linux"
  fi
}

# Determine Install Destination
get_install_dir() {
  local env="$1"
  if [[ "$env" == "termux" ]]; then
    echo "${PREFIX:-/data/data/com.termux/files/usr}/bin"
  elif [[ $EUID -eq 0 ]]; then
    echo "/usr/local/bin"
  else
    echo "${HOME}/.local/bin"
  fi
}

# Detect CPU Architecture
detect_arch() {
  local machine
  machine="$(uname -m)"
  case "${machine}" in
    aarch64|arm64)
      echo "arm64" ;;
    armv7l|armv8l|arm)
      echo "armv7" ;;
    x86_64|amd64)
      echo "x86_64" ;;
    *)
      echo "unsupported" ;;
  esac
}

# Check and install needed CLI tools (curl, tar, sha256sum)
check_dependencies() {
  local env="$1"
  local missing=()
  local deps=("curl" "tar")

  if ! command -v sha256sum &>/dev/null && ! command -v shasum &>/dev/null; then
    missing+=("coreutils")
  fi

  for dep in "${deps[@]}"; do
    command -v "${dep}" &>/dev/null || missing+=("${dep}")
  done

  if [[ ${#missing[@]} -gt 0 ]]; then
    info "Installing missing dependencies: ${missing[*]}"
    if [[ "$env" == "termux" ]]; then
      pkg update -y &>/dev/null || true
      pkg install -y "${missing[@]}"
    elif command -v apt-get &>/dev/null; then
      sudo apt-get update -y && sudo apt-get install -y "${missing[@]}"
    elif command -v dnf &>/dev/null; then
      sudo dnf install -y "${missing[@]}"
    elif command -v brew &>/dev/null; then
      brew install "${missing[@]}"
    else
      warn "Please install missing tools manually: ${missing[*]}"
    fi
  fi
}

# Verify SHA256 checksum
verify_checksum() {
  local file="$1"
  local sha_file="$2"

  if command -v sha256sum &>/dev/null; then
    sha256sum -c "${sha_file}" &>/dev/null
  elif command -v shasum &>/dev/null; then
    shasum -a 256 -c "${sha_file}" &>/dev/null
  else
    warn "sha256sum not found; skipping hash verification."
    return 0
  fi
}

# Map OS and Arch to release archive filename
get_archive_name() {
  local env="$1"
  local arch="$2"

  case "${env}" in
    termux)
      case "${arch}" in
        arm64)  echo "zephyr-android-arm64.tar.gz" ;;
        armv7)  echo "zephyr-android-armv7.tar.gz" ;;
        x86_64) echo "zephyr-android-x86_64.tar.gz" ;;
        *)      echo "" ;;
      esac
      ;;
    macos)
      case "${arch}" in
        arm64)  echo "zephyr-macos-arm64.tar.gz" ;;
        x86_64) echo "zephyr-macos-x86_64.tar.gz" ;;
        *)      echo "" ;;
      esac
      ;;
    linux)
      case "${arch}" in
        arm64)  echo "zephyr-linux-arm64.tar.gz" ;;
        armv7)  echo "zephyr-linux-armv7.tar.gz" ;;
        x86_64) echo "zephyr-linux-x86_64.tar.gz" ;;
        *)      echo "" ;;
      esac
      ;;
  esac
}

# Fetch release asset URL from GitHub API
fetch_asset_url() {
  local archive_name="$1"
  local release_url="${API_BASE}/releases/latest"
  
  local json
  json="$(curl -fsSL -H "Accept: application/vnd.github+json" "${release_url}" 2>/dev/null || true)"
  if [[ -z "${json}" ]]; then
    # Fallback to direct download URL if API is rate-limited or unauthenticated
    echo "https://github.com/${REPO}/releases/latest/download/${archive_name}"
    return 0
  fi

  local download_url
  download_url="$(echo "${json}" | grep -B 2 -A 4 "\"name\": \"${archive_name}\"" | grep "browser_download_url" | head -n 1 | cut -d '"' -f 4 || true)"
  if [[ -n "${download_url}" ]]; then
    echo "${download_url}"
  else
    echo "https://github.com/${REPO}/releases/latest/download/${archive_name}"
  fi
}

# Install / Update Command
do_install() {
  local env arch install_dir archive_name
  env="$(detect_environment)"
  arch="$(detect_arch)"
  install_dir="$(get_install_dir "${env}")"

  info "Detected environment: ${BOLD}${env}${RESET} (${BOLD}${arch}${RESET})"

  if [[ "${arch}" == "unsupported" ]]; then
    error "Architecture $(uname -m) is not supported."
    exit 1
  fi

  archive_name="$(get_archive_name "${env}" "${arch}")"
  if [[ -z "${archive_name}" ]]; then
    error "No prebuilt binary available for ${env} ${arch}."
    exit 1
  fi

  check_dependencies "${env}"

  TMP_DIR="$(mktemp -d /tmp/vpngate-install.XXXXXX 2>/dev/null || mktemp -d -t 'vpngate-install')"
  local archive_path="${TMP_DIR}/${archive_name}"
  local sha_path="${TMP_DIR}/${archive_name}.sha256"

  local asset_url
  asset_url="$(fetch_asset_url "${archive_name}")"
  local sha_url="${asset_url}.sha256"

  info "Downloading ${BOLD}${archive_name}${RESET}..."
  if ! curl -fsSL "${asset_url}" -o "${archive_path}"; then
    error "Failed to download binary archive from ${asset_url}."
    exit 1
  fi

  if curl -fsSL "${sha_url}" -o "${sha_path}" 2>/dev/null; then
    info "Verifying SHA-256 checksum..."
    (cd "${TMP_DIR}" && verify_checksum "${archive_name}" "${archive_name}.sha256") || {
      warn "Checksum mismatch or check skipped. Proceeding..."
    }
  fi

  info "Extracting ${BIN_NAME}..."
  tar -xzf "${archive_path}" -C "${TMP_DIR}"

  mkdir -p "${install_dir}"
  cp "${TMP_DIR}/${BIN_NAME}" "${install_dir}/${BIN_NAME}"
  chmod +x "${install_dir}/${BIN_NAME}"

  if [[ "${env}" == "macos" ]]; then
    xattr -d com.apple.quarantine "${install_dir}/${BIN_NAME}" 2>/dev/null || true
  fi

  success "Installed ${BOLD}${BIN_NAME}${RESET} to ${GREEN}${install_dir}/${BIN_NAME}${RESET}"

  # Check PATH
  if [[ ":$PATH:" != *":${install_dir}:"* ]]; then
    warn "${install_dir} is not in your \$PATH."
    echo -e "Add it to your shell configuration:"
    echo -e "  ${CYAN}export PATH=\"\$PATH:${install_dir}\"${RESET}"
  fi

  echo ""
  echo -e "${BOLD}Quick Usage:${RESET}"
  echo -e "  ${CYAN}${BIN_NAME} check --limit 5${RESET}   # Check reachability from Iran"
  echo -e "  ${CYAN}${BIN_NAME} connect --auto${RESET}     # Auto-connect & listen on 127.0.0.1:1095"
  echo -e "  ${CYAN}${BIN_NAME} --help${RESET}             # Show all commands"
}

# Uninstall Command
do_uninstall() {
  local env install_dir target
  env="$(detect_environment)"
  install_dir="$(get_install_dir "${env}")"
  target="${install_dir}/${BIN_NAME}"

  if [[ -f "${target}" ]]; then
    rm -f "${target}"
    success "Removed ${target}"
  else
    warn "${target} is not installed."
  fi
}

# Run / Connect Command
do_run() {
  local env install_dir target
  env="$(detect_environment)"
  install_dir="$(get_install_dir "${env}")"
  target="${install_dir}/${BIN_NAME}"

  if command -v "${BIN_NAME}" &>/dev/null; then
    exec "${BIN_NAME}" "$@"
  elif [[ -x "${target}" ]]; then
    exec "${target}" "$@"
  else
    warn "${BIN_NAME} not found. Running installation first..."
    do_install
    exec "${install_dir}/${BIN_NAME}" "$@"
  fi
}

show_help() {
  echo -e "${BOLD}Zephyr — Client Manager & Installer${RESET}"
  echo -e "Usage: ./zephyr.sh [command]"
  echo ""
  echo "Commands:"
  echo "  install       Download and install the latest zephyr binary"
  echo "  update        Update to the latest release"
  echo "  uninstall     Remove the installed binary"
  echo "  run [args]    Run zephyr with specified arguments"
  echo "  connect       Quick connect to fastest unblocked server (--auto)"
  echo "  help          Show this help message"
}

case "${1:-install}" in
  install)
    do_install ;;
  update)
    info "Updating zephyr..."
    do_install ;;
  uninstall)
    do_uninstall ;;
  run)
    shift
    do_run "$@" ;;
  connect)
    shift
    do_run connect --auto "$@" ;;
  help|--help|-h)
    show_help ;;
  *)
    # Pass through other arguments directly to the binary
    do_run "$@" ;;
esac
