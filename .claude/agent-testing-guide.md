# Agent Testing and Verification Guide

Reference for all agents on how to see, test, and verify the running Spatia desktop app.

## Prerequisites

The Spatia app must be running for any visual or state verification to work.
All scripts are in the project root under `scripts/`.

```bash
# Check if the app is already running, start it if not (blocks up to 120s)
bash scripts/ensure-app-running.sh
```

If you are running `pnpm tauri dev` yourself in another terminal, the scripts will
detect the existing window and skip launching a new instance.

---

## 1. Seeing the App (All Agents)

### Take a Screenshot

```bash
bash scripts/capture-app.sh
```

This saves two files:
- `scripts/screenshots/latest.png` -- always the most recent capture (overwritten each time)
- `scripts/screenshots/spatia_<YYYYMMDD_HHMMSS>.png` -- timestamped archive copy

To view the screenshot, use the Read tool on the PNG file. You are multimodal and can
interpret the image directly:

```
Read file: /Users/zhaotingzhou/Projects/spatia/scripts/screenshots/latest.png
```

The screenshot captures only the Spatia window (not the full screen). If the Spatia
window is not found, the script exits with code 1 and prints an error.

### Dump UI State to JSON

**Status: requires TASK-P0-2 to be completed.** The `dump-ui-state.sh` script is a
placeholder until the `debug_ui_snapshot` Tauri command and
`window.__spatia_debug_snapshot()` function are implemented.

Once available:

```bash
bash scripts/dump-ui-state.sh
```

This will write JSON to `scripts/ui-state/latest.json` containing the serialized
Zustand store: tables (names, statuses, row counts, address columns), chat messages,
analysis GeoJSON feature count, and any displayed errors.

To read the state:

```
Read file: /Users/zhaotingzhou/Projects/spatia/scripts/ui-state/latest.json
```

### Quick Verification Pattern

The standard "look at the app" workflow for any agent:

```bash
# 1. Make sure it is running
bash scripts/ensure-app-running.sh

# 2. Capture current state
bash scripts/capture-app.sh

# 3. Read the screenshot
# (use Read tool on scripts/screenshots/latest.png)

# 4. Optionally dump structured state (once TASK-P0-2 is done)
bash scripts/dump-ui-state.sh
# (use Read tool on scripts/ui-state/latest.json)
```

---

## 2. Test Engineer: Testing Workflows

### Unit and Integration Tests

```bash
# Run all Rust tests (from project root)
cd src-tauri && cargo test --workspace

# Run tests for a specific crate
cd src-tauri && cargo test -p spatia_engine

# Run a specific test by name
cd src-tauri && cargo test -p spatia_engine -- geocode_batch_uses_local_fuzzy
```

### Full Quality Gate

Run this before declaring any task complete:

```bash
pnpm build && cd src-tauri && cargo test --workspace && cargo clippy --workspace
```

All three commands must pass with zero errors and zero warnings.

### Manual E2E Verification

When you cannot write an automated E2E test (e.g., the WebDriver infrastructure from
TASK-P0-3 is not yet available), use this workflow:

1. **Start the app**:
   ```bash
   bash scripts/ensure-app-running.sh
   ```

2. **Capture baseline state** (before the operation you are testing):
   ```bash
   bash scripts/capture-app.sh scripts/screenshots/before.png
   ```

3. **Trigger the operation** you want to test. Since you cannot click UI elements
   directly, use Tauri commands via the Rust test harness or CLI:
   ```bash
   # Example: ingest a CSV via the CLI
   cd src-tauri && cargo run -p spatia_cli -- ingest ./spatia.duckdb ../data/sample.csv test_table
   ```

4. **Capture after state**:
   ```bash
   bash scripts/capture-app.sh scripts/screenshots/after.png
   ```

5. **Compare**: Read both screenshots to verify the expected change occurred.
   Read the UI state dump (when available) to verify data-level changes.

### Checking for Regressions

Before and after a code change:

```bash
# Before applying changes
bash scripts/capture-app.sh scripts/screenshots/regression_before.png

# ... apply code changes, restart app if needed ...

# After applying changes
bash scripts/capture-app.sh scripts/screenshots/regression_after.png
```

Read both screenshots and verify that existing functionality was not broken.

### Writing Rust Tests

Follow these project conventions:

- **Temp DB files**: Use nanosecond timestamp suffix:
  ```rust
  let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
  let db_path = format!("/tmp/spatia_mytest_{suffix}.duckdb");
  ```
- **Cleanup**: Always remove `.duckdb`, `.wal`, `.wal.lck` in cleanup functions.
- **In-memory**: Use `Connection::open_in_memory()` for logic-only tests.
- **HTTP mocking**: Use `mockito` crate for Geocodio API tests.
- **Async tests**: Use `#[tokio::test]`.
- **Test location**: `#[cfg(test)] mod tests` inside source files, or separate
  `_integration_tests.rs` files included via `#[cfg(test)] mod name;` in `lib.rs`.

---

## 3. Product Manager: Feature Verification

### Verifying a Feature Was Implemented

You cannot interact with the UI directly (no clicking, typing, or dragging). Your
verification tools are:

1. **Screenshots** -- visual confirmation that UI elements exist and look correct
2. **UI state JSON** -- structured confirmation that data is in the expected state
3. **Source code reading** -- confirm the implementation matches acceptance criteria
4. **Rust test output** -- confirm the backend logic is correct

### Verification Workflow

```bash
# 1. Ensure app is running
bash scripts/ensure-app-running.sh

# 2. Take a screenshot
bash scripts/capture-app.sh

# 3. Read the screenshot to verify visual elements
# Look for: component presence, layout correctness, text content, error messages

# 4. Read UI state (when available) for data verification
bash scripts/dump-ui-state.sh
# Check: table count, table statuses, chat message count, feature counts

# 5. Read source code for implementation details
# Key files:
#   src/components/ChatCard.tsx    -- chat UI
#   src/components/FileList.tsx    -- table management panel
#   src/components/MapView.tsx     -- map and overlays
#   src/lib/appStore.ts            -- Zustand store (all app state)
#   src-tauri/src/lib.rs           -- all Tauri commands
```

### What to Look For

**In screenshots:**
- Are expected UI elements visible? (buttons, panels, badges, text)
- Does the layout match the design spec?
- Are error states or empty states rendered correctly?
- Is the map showing data points when expected?

**In UI state JSON:**
- `tables` array: are all expected tables present with correct statuses?
- `chatMessages` array: does the conversation history look right?
- `analysisGeoJson.features`: are the expected number of features present?
- Any unexpected `error` fields on table entries?

### Writing Verifiable Acceptance Criteria

Since agents verify through screenshots and state dumps, write acceptance criteria
that are observable through these tools:

Good (verifiable):
- "The FileList panel shows a table named 'sales_data' with status 'done'"
- "The map displays purple circle markers for geocoded data points"
- "The ChatCard shows an assistant message containing the word 'analysis'"
- "The UI state JSON contains a table with rowCount > 0"

Bad (not verifiable by agents):
- "The animation is smooth" (agents see static screenshots)
- "The hover tooltip shows column details" (agents cannot hover)
- "Clicking the button triggers geocoding" (agents cannot click)

---

## 4. UI Design Architect: Visual Review

### Layout and Styling Review

```bash
# Capture the app at its current state
bash scripts/capture-app.sh

# Read the screenshot
# Examine: spacing, alignment, color usage, typography, component hierarchy
```

### What You Can Verify

- **Layout structure**: panel positions, sizing, z-order (overlays vs base map)
- **Component rendering**: Radix UI components appear correctly styled
- **Color palette**: violet accent, slate gray scale, consistent with Theme config
- **Typography**: text sizes, weights, hierarchy
- **Empty states**: what the app looks like with no data loaded
- **Error states**: visible in screenshots if errors are present
- **Information density**: whether panels feel balanced or cluttered

### What You Cannot Verify

- Hover states, focus rings, transitions, animations (static screenshots only)
- Scroll behavior within panels
- Responsive behavior at different sizes (window size is fixed at 800x600 default)
- Touch/pointer interactions

### Source Code Review for Components

To verify component architecture decisions, read these files:

```
src/App.tsx                    -- top-level layout composition
src/App.css                    -- structural CSS (panel positions, z-index)
src/components/MapView.tsx     -- map container, Deck.gl overlay, GeoJSON layers
src/components/FileList.tsx    -- right panel content, table cards, data preview
src/components/ChatCard.tsx    -- floating chat panel, message rendering
src/lib/appStore.ts            -- Zustand store shape (what state drives UI)
```

The app uses Radix UI Themes with `accentColor="violet"` and `grayColor="slate"`.
All custom CSS uses Radix CSS variables (`--color-panel-solid`, `--gray-a4`, etc.).

---

## 5. Senior Engineer: Development Loop

### Build, Start, Capture, Verify Cycle

```bash
# 1. Make code changes
# 2. Build and verify compilation
pnpm build && cd src-tauri && cargo clippy --workspace

# 3. Start or restart the app
# (if already running, you may need to stop and restart for Rust changes)
bash scripts/ensure-app-running.sh

# 4. Capture and verify
bash scripts/capture-app.sh
# Read scripts/screenshots/latest.png

# 5. Run tests
cd src-tauri && cargo test --workspace

# 6. Full quality gate before declaring done
pnpm build && cd src-tauri && cargo test --workspace && cargo clippy --workspace
```

### Debugging with State Dumps

When a bug is reported or something looks wrong in a screenshot:

```bash
# Capture state for diagnosis (once TASK-P0-2 is done)
bash scripts/dump-ui-state.sh
# Read the JSON to inspect table statuses, error messages, feature counts
```

### Adding New Tauri Commands

When adding a new command:

1. Define the function in `src-tauri/src/lib.rs` with `#[tauri::command]`
2. Register it in the `invoke_handler` array in the `run()` function
3. Call it from the frontend with `invoke<string>("command_name", { args })`
4. Always return `Result<String, String>` with JSON-serialized responses

For debug-only commands, wrap with `#[cfg(debug_assertions)]`.

---

## Appendix: Script Reference

| Script | Purpose | Output |
|--------|---------|--------|
| `scripts/capture-app.sh` | Screenshot the Spatia window | `scripts/screenshots/latest.png` |
| `scripts/capture-app.sh <path>` | Screenshot to specific path | `<path>` |
| `scripts/dump-ui-state.sh` | Dump Zustand store to JSON | `scripts/ui-state/latest.json` (pending TASK-P0-2) |
| `scripts/ensure-app-running.sh` | Start app if not running | Logs to `scripts/logs/tauri-dev.log` |

All scripts exit 0 on success, 1 on failure. Check stderr for error messages.
