#!/bin/sh
# Deezer TUI installer
# Usage: curl -LsSf https://raw.githubusercontent.com/Tatayoyoh/deezer-tui/main/install.sh | sh
set -e

REPO="Tatayoyoh/deezer-tui"
INSTALL_DIR="${HOME}/.local/bin"
BINARY_NAME="deezer-tui"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}" in
    Linux)
        case "${ARCH}" in
            x86_64)  ASSET="${BINARY_NAME}-linux-x86_64" ;;
            aarch64) ASSET="${BINARY_NAME}-linux-aarch64" ;;
            arm64)   ASSET="${BINARY_NAME}-linux-aarch64" ;;
            *)
                echo "Error: unsupported architecture '${ARCH}' for Linux"
                echo "Supported: x86_64, aarch64"
                exit 1
                ;;
        esac
        ;;
    Darwin)
        ASSET="${BINARY_NAME}-macos-universal"
        ;;
    *)
        echo "Error: unsupported OS '${OS}'"
        echo "Supported: Linux, macOS"
        echo "Windows users: use WSL2 with the Linux binary"
        exit 1
        ;;
esac

DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"

echo "Deezer TUI installer"
echo "  OS:           ${OS}"
echo "  Architecture: ${ARCH}"
echo "  Binary:       ${ASSET}"
echo ""

# Create install directory
mkdir -p "${INSTALL_DIR}"

# Download to temp file
TMP_FILE="$(mktemp)"
trap 'rm -f "${TMP_FILE}"' EXIT

echo "Downloading ${DOWNLOAD_URL}..."
if command -v curl >/dev/null 2>&1; then
    curl -fSL --progress-bar -o "${TMP_FILE}" "${DOWNLOAD_URL}"
elif command -v wget >/dev/null 2>&1; then
    wget -q --show-progress -O "${TMP_FILE}" "${DOWNLOAD_URL}"
else
    echo "Error: neither curl nor wget found"
    exit 1
fi

chmod +x "${TMP_FILE}"
mv "${TMP_FILE}" "${INSTALL_DIR}/${BINARY_NAME}"

# Ensure ~/.local/bin is in PATH
add_to_path() {
    SHELL_NAME="$(basename "${SHELL:-/bin/sh}")"
    case "${SHELL_NAME}" in
        bash) RC_FILE="${HOME}/.bashrc" ;;
        zsh)  RC_FILE="${HOME}/.zshrc" ;;
        *)    RC_FILE="" ;;
    esac

    if [ -z "${RC_FILE}" ]; then
        echo "  Add this to your shell config:"
        echo "    export PATH=\"\${HOME}/.local/bin:\${PATH}\""
        return
    fi

    # Check if already in PATH config
    if grep -q '\.local/bin' "${RC_FILE}" 2>/dev/null; then
        return
    fi

    echo '' >> "${RC_FILE}"
    echo '# Added by deezer-tui installer' >> "${RC_FILE}"
    echo 'export PATH="${HOME}/.local/bin:${PATH}"' >> "${RC_FILE}"
    echo "  Updated ${RC_FILE} with PATH entry"
    echo "  Run: source ${RC_FILE}"
}

echo ""
echo "Installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"

# Add to PATH if not already there
case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *) add_to_path ;;
esac

echo "Run 'deezer-tui' to start!"
