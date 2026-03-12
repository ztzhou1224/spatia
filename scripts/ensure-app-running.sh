#!/usr/bin/env bash
# ensure-app-running.sh — Guarantee that the Spatia Tauri dev app is running.
#
# Usage:
#   bash scripts/ensure-app-running.sh
#
# Behaviour:
#   1. Checks whether a Spatia window is already on screen.
#   2. If not, starts `pnpm tauri dev` in the background (logs to
#      scripts/logs/tauri-dev.log).
#   3. Polls for the window to appear, up to WAIT_SECONDS (default 120).
#   4. Exits 0 when the window is detected, exits 1 on timeout.
#
# Exit codes:
#   0 — Spatia window is (or became) visible
#   1 — Timed out waiting for the window

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_DIR="$SCRIPT_DIR/logs"
LOG_FILE="$LOG_DIR/tauri-dev.log"
WAIT_SECONDS="${SPATIA_START_TIMEOUT:-120}"
POLL_INTERVAL=3

mkdir -p "$LOG_DIR"

# ---------------------------------------------------------------------------
# Helper: check whether a Spatia window is currently visible on screen
# Returns exit code 0 if found, 1 if not.
# ---------------------------------------------------------------------------
spatia_window_visible() {
  local wid
  wid=$(osascript -l JavaScript <<'JXAEOF' 2>/dev/null || true
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
  [[ -n "$wid" && "$wid" != "0" ]]
}

# ---------------------------------------------------------------------------
# 1. Check if already running
# ---------------------------------------------------------------------------
if spatia_window_visible; then
  echo "Spatia is already running."
  exit 0
fi

# ---------------------------------------------------------------------------
# 2. Start the dev server in the background
# ---------------------------------------------------------------------------
echo "Spatia is not running. Starting 'pnpm tauri dev' in the background..."
echo "Log output: $LOG_FILE"

# Ensure pnpm is on PATH (Homebrew / nvm / volta installs may not export it)
export PATH="$HOME/.local/share/pnpm:$HOME/.volta/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

nohup bash -c "cd '$PROJECT_ROOT' && pnpm tauri dev" >"$LOG_FILE" 2>&1 &
BG_PID=$!
echo "Background PID: $BG_PID"

# ---------------------------------------------------------------------------
# 3. Poll until the window appears or we time out
# ---------------------------------------------------------------------------
echo "Waiting up to ${WAIT_SECONDS}s for the Spatia window to appear..."
ELAPSED=0
while [[ $ELAPSED -lt $WAIT_SECONDS ]]; do
  if spatia_window_visible; then
    echo "Spatia window detected after ${ELAPSED}s."
    exit 0
  fi

  # Check that the background process is still alive
  if ! kill -0 "$BG_PID" 2>/dev/null; then
    echo "ERROR: 'pnpm tauri dev' process exited unexpectedly." >&2
    echo "       Check the log at: $LOG_FILE" >&2
    exit 1
  fi

  sleep "$POLL_INTERVAL"
  ELAPSED=$(( ELAPSED + POLL_INTERVAL ))
  echo "  ...still waiting (${ELAPSED}s elapsed)"
done

echo "ERROR: Timed out after ${WAIT_SECONDS}s waiting for Spatia to start." >&2
echo "       Check the log at: $LOG_FILE" >&2
exit 1
