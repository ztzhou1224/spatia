#!/usr/bin/env bash
# dump-ui-state.sh — Trigger a UI state snapshot and print the JSON to stdout.
#
# Usage:
#   bash scripts/dump-ui-state.sh [output_path]
#
# How it works:
#   1. Uses osascript (JXA) to call window.__spatia_debug_snapshot() inside the
#      running Spatia webview.  That function serialises the Zustand store and
#      invokes the Tauri `write_debug_snapshot` command, which writes the JSON to
#      scripts/screenshots/ui-state.json.
#   2. This script then reads that file and prints it to stdout (or to the
#      optional output_path argument).
#
# Prerequisites:
#   * Spatia must be running in dev mode (pnpm tauri dev).
#   * The app must have been built with debug_assertions enabled (dev build).
#   * Accessibility permissions may be required for osascript to target the webview.
#
# Exit codes:
#   0  — snapshot captured and printed successfully
#   1  — prerequisite not met, app not found, or output file not produced

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCREENSHOTS_DIR="$SCRIPT_DIR/screenshots"
STATE_FILE="$SCREENSHOTS_DIR/ui-state.json"

OUTPUT_PATH="${1:-}"

mkdir -p "$SCREENSHOTS_DIR"

# Step 1: Invoke window.__spatia_debug_snapshot() via osascript JXA.
# We use a -e string rather than a heredoc to keep the bash syntax clean.
echo "Triggering debug snapshot in Spatia webview..." >&2

JXA_SCRIPT='
var spatia = Application("spatia");
if (!spatia.running()) {
  "ERROR: Spatia is not running";
} else {
  try {
    spatia.windows[0].webViews[0].doJavaScript(
      "if (typeof window.__spatia_debug_snapshot === \"function\") {" +
      "  window.__spatia_debug_snapshot();" +
      "  \"triggered\";" +
      "} else {" +
      "  \"ERROR: __spatia_debug_snapshot not found (not a dev build?)\";" +
      "}"
    );
  } catch(e) {
    "ERROR: " + e.message;
  }
}
'

JXAEOF_RESULT=$(osascript -l JavaScript -e "$JXA_SCRIPT" 2>&1 || true)

if echo "$JXAEOF_RESULT" | grep -qi "error"; then
  echo "WARNING: osascript reported: $JXAEOF_RESULT" >&2
  echo "         Attempting to read the last-written snapshot file anyway..." >&2
fi

# Step 2: Wait briefly for the async Tauri write, then read.
# __spatia_debug_snapshot is async; the Tauri invoke that writes the file is
# awaited inside it, but the JXA call above does not await the JS Promise.
sleep 1

if [[ ! -f "$STATE_FILE" ]]; then
  echo "ERROR: Snapshot file not found at $STATE_FILE" >&2
  echo "       Make sure:" >&2
  echo "         * Spatia is running in dev mode (pnpm tauri dev)" >&2
  echo "         * The app has loaded at least once (Zustand store is initialised)" >&2
  echo "         * This is a debug build (debug_assertions = true)" >&2
  exit 1
fi

# Step 3: Output the JSON.
if [[ -n "$OUTPUT_PATH" ]]; then
  cp "$STATE_FILE" "$OUTPUT_PATH"
  echo "Snapshot written to: $OUTPUT_PATH" >&2
else
  cat "$STATE_FILE"
fi
