#!/usr/bin/env bash
# fili installer — fetch the latest GitHub release tarball and drop the
# binary into ~/.local/bin. Linux x86_64 and aarch64 supported.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/strycore/fili/master/install.sh | bash
#
# Pin a version:
#   curl -fsSL .../install.sh | VERSION=v0.1.0 bash
#
# Different install dir:
#   curl -fsSL .../install.sh | FILI_INSTALL_DIR=$HOME/bin bash

set -euo pipefail

REPO="${FILI_REPO:-strycore/fili}"
INSTALL_DIR="${FILI_INSTALL_DIR:-$HOME/.local/bin}"
BIN_NAME="fili"

err()  { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }
info() { printf '\033[32m›\033[0m %s\n' "$*"; }

arch="$(uname -m)"
os="$(uname -s)"
case "$os $arch" in
  "Linux x86_64"|"Linux amd64")  target="x86_64-linux" ;;
  "Linux aarch64"|"Linux arm64") target="aarch64-linux" ;;
  *) err "no prebuilt binary for $os $arch — build from source: https://github.com/${REPO}" ;;
esac

for cmd in curl tar sed; do
  command -v "$cmd" >/dev/null 2>&1 || err "missing required command: $cmd"
done

if [ -z "${VERSION:-}" ]; then
  info "looking up latest release..."
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | sed -n 's/.*"tag_name":[[:space:]]*"\(v[^"]*\)".*/\1/p' \
    | head -n 1)
  [ -n "$VERSION" ] || err "couldn't resolve latest release for ${REPO}"
fi
version_no_v="${VERSION#v}"

archive="${BIN_NAME}-${version_no_v}-${target}.tar.gz"
url="https://github.com/${REPO}/releases/download/${VERSION}/${archive}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

info "downloading ${archive}..."
curl -fL --progress-bar -o "${tmp}/${archive}" "$url" \
  || err "download failed: $url"

mkdir -p "$INSTALL_DIR"
tar -xzf "${tmp}/${archive}" -C "$tmp"
mv "${tmp}/${BIN_NAME}" "${INSTALL_DIR}/${BIN_NAME}"
chmod +x "${INSTALL_DIR}/${BIN_NAME}"

info "installed ${BIN_NAME} ${version_no_v} → ${INSTALL_DIR}/${BIN_NAME}"

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    printf '\n'
    printf 'Note: %s is not in $PATH.\n' "$INSTALL_DIR"
    printf '  Add to your shell rc:\n'
    printf '    export PATH="%s:$PATH"\n' "$INSTALL_DIR"
    ;;
esac
