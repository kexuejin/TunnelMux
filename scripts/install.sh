#!/usr/bin/env bash
set -euo pipefail

REPO="${TUNNELMUX_REPO:-kexuejin/TunnelMux}"
VERSION="latest"
PREFIX="${TUNNELMUX_PREFIX:-$HOME/.local}"
BIN_DIR=""

usage() {
  cat <<'EOF'
Install TunnelMux binaries from GitHub Releases.

Usage:
  install.sh [--version <tag>] [--prefix <path>] [--bin-dir <path>] [--repo <owner/name>]

Examples:
  install.sh
  install.sh --version v0.1.2
  install.sh --prefix /usr/local
  install.sh --bin-dir /usr/local/bin
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --prefix)
      PREFIX="${2:-}"
      shift 2
      ;;
    --bin-dir)
      BIN_DIR="${2:-}"
      shift 2
      ;;
    --repo)
      REPO="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$BIN_DIR" ]]; then
  BIN_DIR="$PREFIX/bin"
fi

if ! command -v tar >/dev/null 2>&1; then
  echo "tar is required but not found." >&2
  exit 1
fi

download() {
  local url="$1"
  local out="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$out"
    return
  fi
  if command -v wget >/dev/null 2>&1; then
    wget -qO "$out" "$url"
    return
  fi
  echo "curl or wget is required." >&2
  exit 1
}

resolve_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux) os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *)
      echo "Unsupported OS: $os" >&2
      echo "Please download release assets manually from GitHub." >&2
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *)
      echo "Unsupported architecture: $arch" >&2
      echo "Please download release assets manually from GitHub." >&2
      exit 1
      ;;
  esac

  echo "${arch}-${os}"
}

resolve_tag() {
  if [[ "$VERSION" != "latest" ]]; then
    if [[ "$VERSION" == v* ]]; then
      echo "$VERSION"
    else
      echo "v$VERSION"
    fi
    return
  fi

  local latest_url redirect_url tag
  latest_url="https://github.com/${REPO}/releases/latest"
  if command -v curl >/dev/null 2>&1; then
    redirect_url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$latest_url")"
  else
    redirect_url="$(wget --max-redirect=20 --server-response -O /dev/null "$latest_url" 2>&1 | awk '/^  Location: /{print $2}' | tr -d '\r' | tail -n 1)"
  fi

  tag="${redirect_url##*/}"
  if [[ -z "$tag" || "$tag" != v* ]]; then
    echo "Failed to resolve latest release tag from ${latest_url}" >&2
    exit 1
  fi
  echo "$tag"
}

TARGET="$(resolve_target)"
TAG="$(resolve_tag)"
VERSION_NO_V="${TAG#v}"
ASSET="tunnelmux-${VERSION_NO_V}-${TARGET}.tar.gz"
BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

echo "Installing TunnelMux ${TAG} (${TARGET}) from ${REPO}"
download "${BASE_URL}/${ASSET}" "${TMP_DIR}/${ASSET}"
download "${BASE_URL}/SHA256SUMS" "${TMP_DIR}/SHA256SUMS"

if ! grep -q "  ${ASSET}$" "${TMP_DIR}/SHA256SUMS"; then
  echo "Checksum entry for ${ASSET} not found in SHA256SUMS." >&2
  exit 1
fi

(cd "$TMP_DIR" && grep "  ${ASSET}$" SHA256SUMS > SHA256SUMS.asset)
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$TMP_DIR" && sha256sum -c SHA256SUMS.asset)
elif command -v shasum >/dev/null 2>&1; then
  (cd "$TMP_DIR" && shasum -a 256 -c SHA256SUMS.asset)
else
  echo "Warning: sha256sum/shasum not found, skipping checksum verification." >&2
fi

tar -xzf "${TMP_DIR}/${ASSET}" -C "${TMP_DIR}"
PKG_DIR="${TMP_DIR}/tunnelmux-${VERSION_NO_V}-${TARGET}"

if [[ ! -d "$PKG_DIR" ]]; then
  echo "Unexpected archive layout: ${PKG_DIR} not found." >&2
  exit 1
fi

mkdir -p "$BIN_DIR"

for bin in tunnelmuxd tunnelmux-cli; do
  if [[ ! -f "${PKG_DIR}/${bin}" ]]; then
    echo "Missing binary in archive: ${bin}" >&2
    exit 1
  fi
  install -m 0755 "${PKG_DIR}/${bin}" "${BIN_DIR}/${bin}"
done

echo "Installed to ${BIN_DIR}:"
echo "  - tunnelmuxd"
echo "  - tunnelmux-cli"
echo
echo "If '${BIN_DIR}' is not in PATH, add:"
echo "  export PATH=\"${BIN_DIR}:\$PATH\""
