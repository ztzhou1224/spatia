#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
GEOCODER_DIR="${ROOT_DIR}/src-python/spatia-geocoder"
DIST_BIN="${GEOCODER_DIR}/dist/main"
TARGET_DIR="${ROOT_DIR}/src-tauri/binaries"

if [[ $# -ge 1 ]]; then
  DIST_BIN="$1"
fi

if ! command -v rustc >/dev/null 2>&1; then
  HOST_TRIPLE=""
else
  HOST_TRIPLE="$(rustc -vV | awk '/host:/{print $2}')"
fi

if [[ -z "${HOST_TRIPLE}" ]]; then
  OS_NAME="$(uname -s | tr '[:upper:]' '[:lower:]')"
  ARCH_NAME="$(uname -m)"
  case "${ARCH_NAME}" in
    arm64|aarch64)
      ARCH_NAME="aarch64"
      ;;
    x86_64|amd64)
      ARCH_NAME="x86_64"
      ;;
  esac
  case "${OS_NAME}" in
    darwin)
      HOST_TRIPLE="${ARCH_NAME}-apple-darwin"
      ;;
    linux)
      HOST_TRIPLE="${ARCH_NAME}-unknown-linux-gnu"
      ;;
    msys*|mingw*|cygwin*)
      HOST_TRIPLE="${ARCH_NAME}-pc-windows-msvc"
      ;;
    *)
      HOST_TRIPLE="${ARCH_NAME}-${OS_NAME}"
      ;;
  esac
fi

if [[ ! -f "${DIST_BIN}" ]]; then
  echo "dist binary not found: ${DIST_BIN}" >&2
  exit 1
fi

mkdir -p "${TARGET_DIR}"
OUTPUT_NAME="spatia-geocoder-${HOST_TRIPLE}"
cp "${DIST_BIN}" "${TARGET_DIR}/${OUTPUT_NAME}"

echo "sidecar binary written to ${TARGET_DIR}/${OUTPUT_NAME}"
