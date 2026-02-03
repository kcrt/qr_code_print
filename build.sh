#!/usr/bin/env bash
# Usage: ./build.sh (win|mac|linux)

set -euo pipefail

TARGET="${1:-}"
case "$TARGET" in
    win|windows)
        rustup target add x86_64-pc-windows-gnu
        cargo build --release --target x86_64-pc-windows-gnu
        ;;
    mac|darwin|macos)
        # Detect host architecture and add appropriate macOS target
        HOST_ARCH="$(uname -m)"
        case "$HOST_ARCH" in
            arm64|aarch64)
                MAC_TARGET="aarch64-apple-darwin"
                ;;
            x86_64|amd64)
                MAC_TARGET="x86_64-apple-darwin"
                ;;
            *)
                echo "Unknown architecture: $HOST_ARCH" >&2
                exit 1
                ;;
        esac
        # If host is not macOS, install the target
        if [[ "$(uname -s)" != "Darwin" ]]; then
            rustup target add "$MAC_TARGET"
        fi
        cargo build --release --target "$MAC_TARGET"
        ;;
    linux)
        rustup target add x86_64-unknown-linux-gnu
        cargo build --release --target x86_64-unknown-linux-gnu
        ;;
    *)
        echo "Usage: $0 {win|mac|linux}" >&2
        exit 1
        ;;
esac
