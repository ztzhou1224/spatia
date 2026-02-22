#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 4 || $# -gt 7 ]]; then
  echo "usage: $0 <db_path> <table_name> <layer_name> <output_pmtiles> [minzoom] [maxzoom] [drop_rate]" >&2
  echo "example: $0 ./spatia.duckdb overture_places_place places ./out/places.pmtiles 6 14 1" >&2
  exit 1
fi

DB_PATH="$1"
TABLE_NAME="$2"
LAYER_NAME="$3"
OUTPUT_PMtiles="$4"
MINZOOM="${5:-6}"
MAXZOOM="${6:-14}"
DROP_RATE="${7:-1}"

if ! command -v duckdb >/dev/null 2>&1; then
  echo "duckdb CLI not found in PATH" >&2
  exit 1
fi

if ! command -v tippecanoe >/dev/null 2>&1; then
  echo "tippecanoe not found in PATH" >&2
  exit 1
fi

OUT_DIR="$(dirname "$OUTPUT_PMtiles")"
mkdir -p "$OUT_DIR"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

GEOJSONSEQ_PATH="$TMP_DIR/${LAYER_NAME}.geojsonseq"

read -r -d '' EXPORT_SQL <<SQL || true
COPY (
  SELECT
    geom,
    * EXCLUDE (geom)
  FROM ${TABLE_NAME}
  WHERE geom IS NOT NULL
) TO '${GEOJSONSEQ_PATH}'
WITH (FORMAT GDAL, DRIVER 'GeoJSONSeq');
SQL

echo "[1/2] Exporting ${TABLE_NAME} from ${DB_PATH} to GeoJSONSeq..."
duckdb "$DB_PATH" "$EXPORT_SQL"

echo "[2/2] Building PMTiles ${OUTPUT_PMtiles}..."
tippecanoe \
  -o "$OUTPUT_PMtiles" \
  -l "$LAYER_NAME" \
  -P \
  -zg \
  -Z "$MINZOOM" \
  -z "$MAXZOOM" \
  -r "$DROP_RATE" \
  --force \
  "$GEOJSONSEQ_PATH"

echo "done: $OUTPUT_PMtiles"
