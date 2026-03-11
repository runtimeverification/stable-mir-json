#!/usr/bin/env bash
#
# Installs yq (mikefarah/yq) into ~/.local/bin with SHA256 verification.
# Intended for CI runners (Linux amd64).

set -euo pipefail

YQ_VERSION="${YQ_VERSION:-v4.52.4}"
INSTALL_DIR="$HOME/.local/bin"
BASE_URL="https://github.com/mikefarah/yq/releases/download/${YQ_VERSION}"

mkdir -p "$INSTALL_DIR"

wget -qO "$INSTALL_DIR/yq_linux_amd64" "${BASE_URL}/yq_linux_amd64"
wget -qO "$INSTALL_DIR/checksums"       "${BASE_URL}/checksums"

(
    cd "$INSTALL_DIR"
    grep 'yq_linux_amd64$' checksums | sha256sum -c -
    mv yq_linux_amd64 yq
    rm -f checksums
    chmod +x yq
)

echo "$INSTALL_DIR" >> "${GITHUB_PATH:-/dev/null}"
