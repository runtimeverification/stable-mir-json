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
wget -qO "$INSTALL_DIR/checksums-bsd"   "${BASE_URL}/checksums-bsd"

(
    cd "$INSTALL_DIR"
    # checksums-bsd uses BSD-style:   SHA256 (yq_linux_amd64) = 0c4d965e...
    # sha256sum -c expects GNU-style: 0c4d965e...  yq_linux_amd64
    # sed captures the filename (\1) and hash (\2), discarding "SHA256 (", ") = ",
    # then emits them in reversed order to match GNU format.
    grep 'SHA256 (yq_linux_amd64)' checksums-bsd \
        | sed 's/SHA256 (\(.*\)) = \(.*\)/\2  \1/' \
        | sha256sum -c -
    mv yq_linux_amd64 yq
    rm -f checksums-bsd
    chmod +x yq
)

echo "$INSTALL_DIR" >> "${GITHUB_PATH:-/dev/null}"
