#!/usr/bin/env bash
# capture-app.sh — Capture the running Spatia desktop app window to a PNG.
#
# Usage:
#   bash scripts/capture-app.sh [output_path]
#
# If output_path is omitted the screenshot is saved to:
#   scripts/screenshots/latest.png
# A timestamped copy is always written alongside it.
#
# Exit codes:
#   0  — screenshot captured successfully
#   1  — Spatia window not found or screencapture failed

set -euo pipefail

# ---------------------------------------------------------------------------
# Resolve output paths
# ---------------------------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCREENSHOTS_DIR="$SCRIPT_DIR/screenshots"
mkdir -p "$SCREENSHOTS_DIR"

if [[ $# -ge 1 ]]; then
  OUTPUT_PATH="$1"
else
  OUTPUT_PATH="$SCREENSHOTS_DIR/latest.png"
fi

TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
TIMESTAMPED_PATH="$SCREENSHOTS_DIR/spatia_${TIMESTAMP}.png"

# ---------------------------------------------------------------------------
# Find the Spatia window ID via CGWindowListCopyWindowInfo (JXA)
# ---------------------------------------------------------------------------
# We look for any on-screen window whose owner name contains "spatia"
# (case-insensitive). The Tauri app process is typically named "spatia" on
# macOS. If you renamed the binary adjust the pattern below.
echo "Searching for Spatia window..."

WID=$(osascript -l JavaScript <<'JXAEOF' 2>/dev/null || true
ObjC.import("CoreGraphics");
var windows = $.CGWindowListCopyWindowInfo(
  $.kCGWindowListOptionOnScreenOnly,
  $.kCGNullWindowID
);
var arr = ObjC.unwrap(windows);
var result = "";
for (var i = 0; i < arr.length; i++) {
  var entry = ObjC.unwrap(arr[i]);
  var owner = entry.kCGWindowOwnerName;
  if (owner) {
    owner = ObjC.unwrap(owner);
    if (owner && owner.toLowerCase().includes("spatia")) {
      var wnum = entry.kCGWindowNumber;
      if (wnum !== undefined) {
        result = String(ObjC.unwrap(wnum));
        break;
      }
    }
  }
}
result;
JXAEOF
)

if [[ -z "$WID" || "$WID" == "0" ]]; then
  echo "ERROR: Could not find a running Spatia window." >&2
  echo "       Make sure the Spatia app is running (pnpm tauri dev or the built app)." >&2
  exit 1
fi

echo "Found Spatia window ID: $WID"

# ---------------------------------------------------------------------------
# Capture the window
# ---------------------------------------------------------------------------
# -l  capture a specific window by ID
# -x  suppress the camera shutter sound
# -o  do not include window shadow in the capture
if screencapture -l"$WID" -x -o "$OUTPUT_PATH" 2>/dev/null; then
  echo "Screenshot saved to: $OUTPUT_PATH"
else
  echo "ERROR: screencapture failed for window ID $WID." >&2
  exit 1
fi

# Write the timestamped copy (copy after capture to avoid running screencapture twice)
cp "$OUTPUT_PATH" "$TIMESTAMPED_PATH"
echo "Timestamped copy:    $TIMESTAMPED_PATH"
