#!/usr/bin/env bash
# Download DuckDB extensions for offline / network-restricted environments.
#
# Usage:
#   bash src-tauri/scripts/download_duckdb_extensions.sh
#
# This downloads the spatial and httpfs extensions for the DuckDB version
# used by this project and stores them in ~/.duckdb/extensions/ so that
# LOAD spatial / LOAD httpfs succeeds without network access.
#
# Run this on a machine WITH internet access before working in a
# network-restricted environment (e.g. Claude VM, air-gapped CI).

set -euo pipefail

DUCKDB_VERSION="${DUCKDB_VERSION:-v1.4.4}"
PLATFORM="${DUCKDB_PLATFORM:-linux_amd64}"
EXTENSIONS=("spatial" "httpfs")

BASE_URL="https://extensions.duckdb.org/${DUCKDB_VERSION}/${PLATFORM}"
DEST_DIR="${HOME}/.duckdb/extensions/${DUCKDB_VERSION}/${PLATFORM}"

mkdir -p "${DEST_DIR}"

for ext in "${EXTENSIONS[@]}"; do
    url="${BASE_URL}/${ext}.duckdb_extension.gz"
    dest="${DEST_DIR}/${ext}.duckdb_extension"

    if [ -f "${dest}" ]; then
        echo "[skip] ${ext} already exists at ${dest}"
        continue
    fi

    echo "[download] ${ext} from ${url}"
    curl -fSL "${url}" | gunzip > "${dest}"
    echo "[ok] ${ext} installed to ${dest}"
done

echo ""
echo "Extensions installed to: ${DEST_DIR}"
echo "DuckDB will now use these cached extensions without network access."
