# Spatia Development Plan - MVP Sprint

## Project Assessment (2026-03-09)

### Current State Summary

**Quality gates: ALL PASSING**
- `pnpm build` -- passes (one known bundler warning from @loaders.gl, non-blocking)
- `cargo test --workspace` -- 59 tests pass, 0 failures
- `cargo clippy --workspace` -- clean, no warnings

**What Works End-to-End:**
1. CSV ingestion with auto table naming (multi-file support)
2. AI-powered data cleaning via Gemini (iterative rounds of UPDATE statements)
3. Address column detection (heuristic on column names + types)
4. Geocoding pipeline: local Overture fuzzy match -> Geocodio HTTP fallback -> persistent cache
5. Unified chat_turn command: multi-table schema injection -> Gemini JSON response -> SQL execution -> GeoJSON + map_actions
6. Map rendering: MapLibre + PMTiles sources + Deck.gl scatter overlay + analysis result GeoJSON layers (point/line/polygon)
7. Map actions from AI: fly_to, fit_bounds, show_popup, highlight_features
8. Table management: list, preview (50 rows), delete
9. Overture extract/search/geocode commands (CLI + Tauri)

**Architecture Health:**
- Clean 3-component frontend: MapView (full viewport), FileList (right panel), ChatCard (floating bottom-left)
- Zustand store (appStore) is simple and well-structured with tables, chatMessages, analysisGeoJson
- Rust engine is well-modularized: ingest, schema, analysis, geocode, overture, identifiers
- AI crate cleanly separated: client, prompts, cleaner -- feature-gated behind `gemini`
- SQL safety: identifier validation in identifiers.rs, analysis SQL prefix enforcement, cleaner UPDATE-only allowlist

**What Is Partially Built / Has Known Gaps:**
1. **No auto-geocode in pipeline** -- `ingest_file_pipeline` detects address columns but stops at "ready" status; user must manually click "Geocode". The MVP flow says "System geocodes addresses if needed (automatically detects address columns)."
2. **No data shown on map after ingestion** -- geocoded data with _lat/_lon columns is not automatically rendered on the map. Only analysis_result GeoJSON shows up.
3. **Chat needs tables to function** -- if no tables are loaded, the chat sends an empty table list and will fail or produce unhelpful responses.
4. **No table selection for chat context** -- all "done" or "ready" tables are sent to chat. No way for user to focus on specific table(s).
5. **Visualization types limited** -- only scatter Deck.gl overlay implemented; heatmap and hexbin are in prompt but not rendered.
6. **No error UX for missing API keys** -- if SPATIA_GEMINI_API_KEY is missing, AI features silently degrade; user sees no guidance.
7. **Bundler warning** -- @loaders.gl "spawn" import warning (non-blocking but should be resolved).
8. **summary.md references deleted files** -- Core Paths still mentions src/pages, widgetStore.ts, useFocusGuard.ts, aiContext.ts which are all deleted.

**What Is Missing for MVP:**
1. **Auto-geocode after ingestion** -- seamless "ingest -> clean -> detect -> geocode" pipeline
2. **Auto-display geocoded data on map** -- after geocoding, points should appear on the map without requiring a chat query
3. **Onboarding / empty state** -- first-time user has no guidance on what to do
4. **Data table / results view** -- chat answers are text-only; no tabular result display for non-spatial queries
5. **Error handling polish** -- missing API key messaging, geocoding failures, network errors

### Critical Path to MVP

The MVP user flow is: **Upload CSV -> Auto-process -> Ask questions -> See results on map/table**

The critical path is making this flow work seamlessly end-to-end:

1. Auto-geocode after address detection (remove the manual "Geocode" button step)
2. Auto-display geocoded data on map after pipeline completes
3. Polish chat UX so results are useful (show SQL, show tabular data, show row counts)
4. Handle the "no API key" case gracefully with clear messaging
5. Fix stale documentation

---

## Phase 0: Agent Testing and Visibility Infrastructure (Priority: CRITICAL -- enables quality for all other phases)

Goal: Give all agents the ability to see and test the running Tauri desktop app.
This is foundational infrastructure -- without it, the test-engineer cannot do E2E testing
and the product-manager/ui-design-architect cannot verify features.

### Problem: macOS Tauri Testing Gap

Tauri v2's official WebDriver support does NOT work on macOS (only Linux/Windows). macOS
uses WKWebView, and Apple provides no WebDriver for embedded WKWebView apps. However:

1. A community crate `tauri-plugin-webdriver-automation` (Feb 2026) provides W3C WebDriver
   specifically for Tauri WKWebView on macOS.
2. macOS native `screencapture -l<WID>` can capture any window by ID without permissions issues.
3. Tauri's `webview.eval()` can execute arbitrary JS to dump DOM state from inside the app.

### Approach: Two-Layer Strategy

**Layer 1 (Simple, no dependencies):** Shell scripts that capture screenshots and dump UI state
from the running app. Any agent can invoke these via Bash. This covers the product-manager and
ui-design-architect use case (visual verification).

**Layer 2 (Richer, requires plugin):** The `tauri-plugin-webdriver-automation` crate for
full WebDriver E2E tests. This covers the test-engineer use case (automated interaction testing).

### TASK-P0-1: App screenshot and UI state capture scripts (est: 3h, role: senior-engineer)
- **Description**: Create shell scripts that any agent can call to capture the current visual
  state of the running Spatia app. Agents are multimodal -- they can read PNG files natively
  via their Read tool. This gives product-manager and ui-design-architect "eyes" on the app.
- **Deliverables**:
  1. `scripts/capture-app.sh` -- Captures the Spatia window to `screenshots/<timestamp>.png`
     - Uses Swift + CoreGraphics `CGWindowListCopyWindowInfo` to find Spatia's window ID by process name
     - Uses `screencapture -l<WID> -x` to capture without sound
     - Falls back to full-screen capture if window not found
     - Outputs the file path so agents can `Read` it
  2. `scripts/dump-ui-state.sh` -- Dumps current UI state to `screenshots/<timestamp>-state.json`
     - Hits the Vite dev server at `http://localhost:1420` using curl or a small Node script
     - Extracts: page title, visible text content, table count, chat message count, etc.
     - Alternative approach: inject JS via a Tauri command that returns DOM summary
  3. `scripts/ensure-app-running.sh` -- Checks if Spatia is running, starts it if not
     - Checks for the Tauri dev process on port 1420
     - Starts `pnpm tauri dev` in background if needed
     - Waits for the dev server to be ready (polls localhost:1420)
- **Acceptance criteria**:
  - Running `bash scripts/capture-app.sh` while Spatia is open produces a readable PNG
  - Running `bash scripts/dump-ui-state.sh` produces JSON with current app state
  - Agents can call these scripts via Bash tool and then Read the output files
  - Works on macOS (darwin) -- that is our only dev platform
- **Implementation notes**:
  - The screenshot approach is verified to work: `screencapture -l<WID> -x <file>` with
    WID obtained via Swift `CGWindowListCopyWindowInfo` filtering by `kCGWindowOwnerName`
  - For UI state dump, the simplest approach is a new Tauri command `dump_ui_state` that
    runs `webview.eval()` to extract DOM info, OR a standalone Node script that uses
    the Tauri dev server URL. The Tauri command approach is better because it works with
    the actual app state (Zustand store), not just the HTML.
- **Files**: `scripts/capture-app.sh`, `scripts/dump-ui-state.sh`, `scripts/ensure-app-running.sh`
- **Dependencies**: None

### TASK-P0-2: Tauri command for UI state introspection (est: 2h, role: senior-engineer)
- **Description**: Add a debug-only Tauri command that returns a JSON snapshot of the current
  application state. This is the "backend" for `dump-ui-state.sh` and also useful for
  automated testing assertions.
- **Deliverables**:
  - New Tauri command `debug_ui_snapshot` (only registered in debug builds via `#[cfg(debug_assertions)]`)
  - Returns JSON with: loaded tables (names, statuses, row counts), chat message count,
    analysis GeoJSON feature count, map center/zoom, any errors displayed
  - Frontend counterpart: a global `window.__spatia_debug_snapshot()` function exposed in
    dev mode that serializes the Zustand store state
- **Acceptance criteria**:
  - `invoke("debug_ui_snapshot")` returns complete app state JSON in dev builds
  - Not available in release builds (compiled out)
  - `dump-ui-state.sh` calls this command and writes the result to a file
- **Files**: `src-tauri/src/lib.rs`, `src/lib/appStore.ts` (expose debug fn), `scripts/dump-ui-state.sh`
- **Dependencies**: None

### TASK-P0-3: WebDriver E2E test infrastructure (est: 4h, role: senior-engineer)
- **Description**: Set up `tauri-plugin-webdriver-automation` for full WebDriver E2E testing
  on macOS. This enables the test-engineer to write automated tests that launch the app,
  interact with UI elements, and assert on results.
- **Deliverables**:
  1. Add `tauri-plugin-webdriver-automation` to `src-tauri/Cargo.toml` (debug-only)
  2. Register plugin in lib.rs `run()` under `#[cfg(debug_assertions)]`
  3. Install CLI: `cargo install tauri-webdriver-automation` (documents in README)
  4. Create `tests/e2e/` directory with WebDriverIO config and one smoke test
  5. Smoke test: launch app, verify title is "spatia", verify map container exists,
     take a screenshot, verify FileList component renders
  6. Document in `tests/e2e/README.md`: how to run, prerequisites, troubleshooting
- **Acceptance criteria**:
  - `tauri-wd --port 4444` starts successfully
  - `npx wdio run tests/e2e/wdio.conf.mjs` executes and passes the smoke test
  - Test can find elements, click buttons, read text, and take screenshots
  - Screenshots saved to `tests/e2e/screenshots/`
- **Files**: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `tests/e2e/*`
- **Dependencies**: TASK-P0-2 (for state assertions)
- **Risk**: The `tauri-plugin-webdriver-automation` crate is community-maintained (Feb 2026).
  If it does not work reliably, fall back to Layer 1 only (screenshot + state dump scripts)
  which are already sufficient for most verification needs.

### TASK-P0-4: Agent workflow documentation (est: 1h, role: gis-tech-lead)
- **Description**: Document how each agent type uses the testing infrastructure. This is
  reference documentation that agents consult when they need to verify their work.
- **Deliverables**: `docs/agent-testing-guide.md` covering:
  - **test-engineer**: How to write and run E2E tests with WebDriverIO, how to use
    `debug_ui_snapshot` for assertions, screenshot comparison workflow
  - **product-manager**: How to visually verify features using `capture-app.sh` + Read tool,
    how to read UI state dumps, how to do blackbox acceptance testing
  - **ui-design-architect**: How to verify layout/styling using screenshots, how to check
    component rendering, how to verify responsive behavior at different window sizes
  - **senior-engineer**: How to run the full test suite before merge, how to add new E2E tests
- **Files**: `docs/agent-testing-guide.md`
- **Dependencies**: TASK-P0-1, TASK-P0-2, TASK-P0-3

---

## Phase 1: Complete the Automated Pipeline (Priority: CRITICAL)

Goal: Make "Upload CSV -> data appears on map" work without manual steps.

### TASK-01: Auto-geocode in ingest_file_pipeline (est: 3h, role: senior-engineer)
- **Description**: Extend `ingest_file_pipeline` Tauri command to automatically geocode the first detected address column after AI cleaning. Currently, the pipeline stops at detection and sets status to "ready". It should continue through geocoding and set status to "done".
- **Acceptance criteria**:
  - If address columns are detected, pipeline automatically geocodes using the first column
  - Progress events emitted for geocoding stage
  - If geocoding fails (e.g., no API key, no Overture tables), pipeline still completes with status "done" but includes a warning
  - If no address columns detected, pipeline completes as-is
- **Files**: `src-tauri/src/lib.rs` (ingest_file_pipeline fn), `src/components/FileList.tsx` (update status handling)
- **Dependencies**: None

### TASK-02: Auto-display geocoded data on map (est: 4h, role: senior-engineer)
- **Description**: After a table reaches "done" status with _lat/_lon columns, automatically render its points on the map. Currently, only `analysis_result` GeoJSON is displayed.
- **Acceptance criteria**:
  - New Tauri command `table_to_geojson` that converts a table with _lat/_lon to GeoJSON FeatureCollection (limit 10000 features)
  - After pipeline completion, FileList triggers map data load
  - Points from ingested tables rendered as a distinct layer (different color from analysis results)
  - Multiple tables can have their points shown simultaneously
- **Files**: `src-tauri/src/lib.rs` (new command), `src/lib/appStore.ts` (table geojson state), `src/components/MapView.tsx` (new layer), `src/components/FileList.tsx` (trigger on done)
- **Dependencies**: TASK-01

### TASK-03: Graceful degradation without API keys (est: 2h, role: senior-engineer)
- **Description**: Show clear user-facing messages when SPATIA_GEMINI_API_KEY or SPATIA_GEOCODIO_API_KEY are missing. Currently these fail silently or return cryptic errors.
- **Acceptance criteria**:
  - New Tauri command `check_api_config` that returns which keys are configured
  - FileList shows a banner if no Gemini key (AI cleaning will be skipped)
  - ChatCard shows inline message if no Gemini key
  - Geocoding gracefully falls back when no Geocodio key (Overture local-only mode)
- **Files**: `src-tauri/src/lib.rs`, `src/components/FileList.tsx`, `src/components/ChatCard.tsx`
- **Dependencies**: None

---

## Phase 2: Chat UX Polish (Priority: HIGH)

Goal: Make AI chat responses useful and informative.

### TASK-04: Tabular results display in chat (est: 4h, role: senior-engineer)
- **Description**: When chat_turn returns SQL results, show a compact data table beneath the message in addition to (or instead of) just the row count. Many analysis questions produce tabular answers (aggregations, top-N) that are not spatial.
- **Acceptance criteria**:
  - ChatCard renders an inline table for analysis results (max 20 rows, scrollable)
  - Table shows column headers and values
  - Non-spatial results (no geometry) show table only, no map update
  - Spatial results show both table and map
- **Files**: `src/components/ChatCard.tsx`, possibly `src-tauri/src/lib.rs` (extend chat_turn response with rows)
- **Dependencies**: None

### TASK-05: Conversation context and table selection (est: 3h, role: senior-engineer)
- **Description**: Allow user to select which table(s) the chat operates on, rather than always sending all tables. For users with many tables, this focuses the AI and reduces prompt size.
- **Acceptance criteria**:
  - FileList table cards have a "chat context" toggle (checkbox or similar)
  - ChatCard shows which tables are in context (pill badges)
  - Default: all "done" tables are in context
  - appStore tracks selectedTablesForChat
- **Files**: `src/lib/appStore.ts`, `src/components/FileList.tsx`, `src/components/ChatCard.tsx`
- **Dependencies**: None

### TASK-06: Chat history clear and conversation management (est: 2h, role: senior-engineer)
- **Description**: Add ability to clear chat history and start a new conversation. Currently messages accumulate forever and the conversation history grows unbounded.
- **Acceptance criteria**:
  - "New chat" button in ChatCard that calls clearMessages()
  - Chat automatically summarizes or truncates history beyond 20 messages (already limited to 10 in prompt, but store grows)
  - Visual separator between conversations
- **Files**: `src/components/ChatCard.tsx`, `src/lib/appStore.ts`
- **Dependencies**: None

---

## Phase 3: Robustness and Quality (Priority: MEDIUM)

### TASK-07: Fix stale documentation (est: 1h, role: senior-engineer)
- **Description**: Update summary.md Core Paths section to reflect the new component structure. Remove references to deleted files (widgetStore.ts, useFocusGuard.ts, aiContext.ts, src/pages/*).
- **Acceptance criteria**:
  - summary.md accurately reflects current file structure
  - CLAUDE.md focus/context system section updated to reflect new appStore approach
- **Files**: `summary.md`, `CLAUDE.md`
- **Dependencies**: None

### TASK-08: Resolve bundler warning (est: 2h, role: senior-engineer)
- **Description**: The @loaders.gl/worker-utils "spawn" import warning should be resolved or suppressed. This is a Vite/Rollup issue with deck.gl's dependency tree trying to import Node.js `child_process.spawn` in a browser context.
- **Acceptance criteria**:
  - Build completes without the spawn warning
  - Deck.gl ScatterplotLayer still works correctly
  - Documented in vite.config.ts if workaround is needed
- **Files**: `vite.config.ts`
- **Dependencies**: None

### TASK-09: Harden SQL execution safety (est: 3h, role: senior-engineer)
- **Description**: The analysis SQL validator only checks the prefix. Add a basic blocklist check for dangerous SQL patterns (DROP, TRUNCATE, DELETE, ALTER, GRANT, etc.) anywhere in the query body.
- **Acceptance criteria**:
  - Analysis SQL rejects statements containing DROP TABLE, TRUNCATE, DELETE FROM, ALTER TABLE, GRANT, REVOKE as substrings (case-insensitive)
  - Existing tests still pass
  - New tests for blocked patterns
- **Files**: `src-tauri/crates/engine/src/analysis.rs`
- **Dependencies**: None

### TASK-10: Expand visualization types (est: 4h, role: senior-engineer)
- **Description**: Add heatmap and hexbin Deck.gl layers to MapView. The AI prompt already suggests these types but only scatter is rendered.
- **Acceptance criteria**:
  - chat_turn response can specify visualization type
  - MapView renders HeatmapLayer for "heatmap" type
  - MapView renders HexagonLayer for "hexbin" type
  - Falls back to scatter for unknown types
- **Files**: `src/components/MapView.tsx`, `src/lib/appStore.ts` (add visualizationType), `package.json` (may need @deck.gl/aggregation-layers)
- **Dependencies**: None

---

## Phase 4: Polish and Testing (Priority: LOW for MVP, HIGH for release)

### TASK-11: Integration tests for Tauri analysis commands (est: 4h, role: test-engineer)
- **Description**: Write integration tests for analysis_chat, generate_analysis_sql, execute_analysis_sql, and the new chat_turn command. These should test the full flow without requiring a real Gemini API key.
- **Files**: `src-tauri/crates/engine/src/analysis.rs` (unit tests), `src-tauri/src/lib.rs` (integration test patterns)
- **Dependencies**: TASK-09

### TASK-12: Empty state and onboarding UX (est: 3h, role: ui-design-architect)
- **Description**: Design and implement helpful empty states for when no data is loaded. Guide the user through their first upload.
- **Acceptance criteria**:
  - Map shows a centered welcome overlay when no tables exist
  - FileList empty state has clear call-to-action
  - ChatCard shows contextual hint about adding data first
- **Files**: `src/components/MapView.tsx`, `src/components/FileList.tsx`, `src/components/ChatCard.tsx`
- **Dependencies**: None

### TASK-13: Code-split large bundle (est: 2h, role: senior-engineer)
- **Description**: The production JS bundle is 2MB (575KB gzipped). Split maplibre-gl and deck.gl into separate chunks via dynamic imports.
- **Files**: `vite.config.ts`, potentially lazy-load MapView
- **Dependencies**: None

---

## Sprint Status

**COMPLETED:**
- [x] TASK-P0-1: Screenshot + UI state capture scripts
- [x] TASK-P0-2: Tauri debug_ui_snapshot command
- [x] TASK-P0-4: Agent testing guide (.claude/agent-testing-guide.md)
- [x] TASK-01: Auto-geocode in ingest_file_pipeline
- [x] TASK-02: Auto-display geocoded data on map (table_to_geojson + blue scatter layer)
- [x] TASK-03: Graceful degradation without API keys (check_api_config + banners)
- [x] TASK-04: Tabular results display in chat (ResultTable component)
- [x] TASK-05: Table selection for chat context (selectedTablesForChat + toggles)
- [x] TASK-06: Chat history clear / new chat button + 50 message cap
- [x] TASK-07: Fix stale documentation (summary.md updated)
- [x] TASK-08: Resolve bundler warning (child_process shim)
- [x] TASK-09: Harden SQL safety (15-pattern blocklist with word-boundary regexes)
- [x] TASK-10: Expand visualization types (heatmap + hexbin Deck.gl layers)
- [x] TASK-11: Integration tests for analysis (8 test cases + SQL safety tests)
- [x] TASK-12: Empty state / onboarding UX (map welcome overlay, FileList, ChatCard)
- [x] TASK-13: Code-split large bundle (manualChunks for maplibre + deckgl)

**DEFERRED:**
- [ ] TASK-P0-3: WebDriver E2E test infrastructure (community crate risk — using Layer 1 for now)

---
---

# Post-MVP: Insurance Vertical Sprint Plan

**Date:** 2026-03-14
**Context:** Following the market-fit-analysis, Spatia is pivoting from generic desktop GIS to **vertical spatial intelligence for insurance underwriting**. Core technology unchanged; positioning, data integrations, AI prompts, and workflow design now target property risk assessment for small-to-mid insurance carriers and MGAs.

**Strategic rationale:** Insurance underwriting is the strongest vertical because (1) bad risk assessment costs insurers millions, (2) local-first becomes a compliance feature for sensitive policy data, (3) the existing CSV→geocode→analyze→map pipeline maps directly to underwriting workflows, and (4) incumbent tools (SpatialKey/Insurity) cost $100K+/year — Spatia can compete at 1/10th the price.

---

## Phase 1: Table Stakes (Pre-Launch Blockers)

Goal: Ship the minimum capabilities required for any professional user to complete a workflow end-to-end. Without these, Spatia cannot be positioned as a production tool.

### TASK-14: CSV export of any table (est: 3h, role: senior-engineer)
- **Description**: Add a "Download CSV" button to each table card in the FileList panel. Users must be able to export any loaded table (raw, geocoded, or analysis_result) as a CSV file.
- **Approach**: New Tauri command `export_table_csv` that runs `COPY <table> TO '<path>' (FORMAT CSV, HEADER)` via DuckDB. Frontend uses Tauri's save dialog (`dialog.save`) for file path selection.
- **Acceptance criteria**:
  - Each table card in FileList shows a download/export icon button
  - Clicking opens a native save dialog defaulting to `<table_name>.csv`
  - Exported CSV includes headers and all rows
  - Works for regular tables, geocoded tables, and the `analysis_result` view
- **Files**: `src-tauri/src/lib.rs` (new command), `src-tauri/crates/engine/src/export.rs` (new module), `src/components/FileList.tsx` (export button)
- **Dependencies**: None

### TASK-15: GeoJSON export of analysis_result (est: 2h, role: senior-engineer)
- **Description**: Add "Export GeoJSON" button for the current analysis result. This enables users to share spatial outputs with other GIS tools.
- **Approach**: New Tauri command `export_analysis_geojson` that serializes the current `analysis_result` view to GeoJSON FeatureCollection and saves via native dialog.
- **Acceptance criteria**:
  - Export button appears in ChatCard when analysis results exist
  - Exports valid GeoJSON FeatureCollection with all properties
  - File saved via native save dialog defaulting to `analysis_result.geojson`
- **Files**: `src-tauri/src/lib.rs`, `src-tauri/crates/engine/src/export.rs`, `src/components/ChatCard.tsx`
- **Dependencies**: TASK-14 (shared export module)

### TASK-16: Map PNG export (est: 2h, role: senior-engineer)
- **Description**: Add "Export Map" button to MapView toolbar that captures the current map viewport as a PNG image.
- **Approach**: Use `map.getCanvas().toDataURL('image/png')` on the MapLibre instance, then pass the base64 data to a Tauri command that writes it to disk via save dialog.
- **Acceptance criteria**:
  - Export button visible in map toolbar/controls area
  - Captures full viewport including all Deck.gl overlays and base map
  - Saved as PNG via native save dialog
  - Works with all basemap types and layer combinations
- **Files**: `src/components/MapView.tsx`, `src-tauri/src/lib.rs` (save file command)
- **Dependencies**: None

### TASK-17: Settings UI — API key management (est: 4h, role: senior-engineer)
- **Description**: Build a settings panel accessible from the app toolbar. Users must be able to enter, update, and verify API keys (Gemini, Geocodio) without touching environment variables. Keys stored via Tauri's secure storage plugin.
- **Approach**: New `SettingsPanel` component (modal or slide-over). Use `tauri-plugin-store` or `tauri-plugin-stronghold` for secure key storage. New Tauri commands: `save_api_key`, `get_api_key`, `verify_api_key`. At startup, check secure storage before falling back to env vars.
- **Acceptance criteria**:
  - Settings gear icon in the app header/toolbar opens the settings panel
  - Fields for Gemini API key and Geocodio API key (masked input)
  - "Test" button that verifies each key responds (ping the API)
  - Keys persisted across sessions via Tauri secure storage
  - Env vars still work as fallback (backward compatible)
  - PMTiles file picker (native file dialog) to select local tile files
- **Files**: `src/components/SettingsPanel.tsx` (new), `src/App.tsx` (mount settings), `src-tauri/src/lib.rs` (key commands), `src-tauri/Cargo.toml` (secure storage plugin)
- **Dependencies**: None

### TASK-18: Map legend — auto-generated from active layer (est: 3h, role: senior-engineer)
- **Description**: Add an auto-generated legend overlay to MapView that reflects the current active Deck.gl layer type, color encoding, and data source.
- **Approach**: New `MapLegend` component rendered as a positioned overlay inside MapView. Reads `visualizationType`, layer color config, and data source name from appStore. Renders appropriate legend items: color gradient for heatmap, color stops for hexbin, single color for scatter.
- **Acceptance criteria**:
  - Legend appears when any Deck.gl overlay is active
  - Shows layer type name, color scale, and data source table name
  - For quantitative scales (heatmap, hexbin): shows min/max range
  - For scatter: shows point color and label
  - Legend hides when no overlay is active
  - Positioned bottom-left or top-right, non-overlapping with other controls
- **Files**: `src/components/MapLegend.tsx` (new), `src/components/MapView.tsx` (mount legend), `src/lib/appStore.ts` (legend state if needed)
- **Dependencies**: None

### TASK-19: Basemap selector (est: 2h, role: senior-engineer)
- **Description**: Add a basemap selector control to the map. Minimum options: CartoDB Dark Matter, CartoDB Positron (light), and OpenStreetMap.
- **Approach**: New `BasemapSelector` component (small floating button group or dropdown) in MapView. On selection, update the MapLibre style URL. Store selection in appStore for persistence.
- **Acceptance criteria**:
  - Basemap selector visible on the map (floating control)
  - Three options minimum: Dark, Light, OpenStreetMap
  - Switching basemaps preserves current viewport (center, zoom)
  - Preserves all Deck.gl overlays and data layers
  - Selection persists across sessions (localStorage or appStore)
- **Files**: `src/components/BasemapSelector.tsx` (new), `src/components/MapView.tsx` (mount selector), `src/lib/appStore.ts` (basemap state)
- **Dependencies**: None
- **Note**: Already listed in architecture.md as implemented basemaps — verify current state before starting. If partially done, extend rather than rebuild.

### TASK-20: Truncation indicators on map and table (est: 2h, role: senior-engineer)
- **Description**: When results are capped (1,000 GeoJSON features, 20 table rows), show an explicit "Showing X of Y" badge. Silent truncation destroys analytical trust.
- **Approach**: Extend analysis SQL execution to return total row count alongside truncated results (run `SELECT COUNT(*) FROM analysis_result` before truncation). Display badge on map overlay and in ResultTable header.
- **Acceptance criteria**:
  - Map shows "Showing X of Y features" badge when GeoJSON is truncated
  - ResultTable shows "Showing X of Y rows" in header when rows are truncated
  - Badge only appears when truncation actually occurs
  - Total count is accurate (from COUNT(*) query)
- **Files**: `src-tauri/src/lib.rs` (return total count), `src/components/MapView.tsx` (badge), `src/components/ChatCard.tsx` (table badge)
- **Dependencies**: None

### TASK-21: Tooltip labels on all UI controls (est: 2h, role: senior-engineer)
- **Description**: Add descriptive tooltip labels to all icon-only buttons across the UI. Currently, many controls are unlabeled icons that are not discoverable.
- **Approach**: Audit all icon buttons in MapView, FileList, ChatCard, and any other components. Add Radix UI `Tooltip` wrappers with descriptive labels.
- **Acceptance criteria**:
  - Every icon-only button has a hover tooltip describing its function
  - Tooltips use consistent styling (Radix UI Tooltip component)
  - Labels are concise and action-oriented (e.g., "Export CSV", "New Chat", "Toggle Layer")
- **Files**: `src/components/MapView.tsx`, `src/components/FileList.tsx`, `src/components/ChatCard.tsx`
- **Dependencies**: None

---

## Phase 2: Competitive Parity

Goal: Bring Spatia to a level where direct comparison against Felt, Kepler.gl, and lighter ArcGIS/Carto use cases is favorable.

### TASK-22: GeoJSON and Shapefile import (est: 4h, role: senior-engineer)
- **Description**: Extend the ingest pipeline to accept `.geojson` and `.shp` files in addition to CSV. Without polygon data, spatial joins and geographic aggregations are impossible.
- **Approach**: DuckDB spatial extension supports `ST_Read()` for GeoJSON and Shapefile (via GDAL bindings). Extend `ingest_csv_with_progress` to detect file extension and route to appropriate DuckDB load command. Geometry columns stored as DuckDB GEOMETRY type.
- **Acceptance criteria**:
  - FileList upload accepts `.geojson`, `.json`, and `.shp` files (plus `.dbf`/`.shx`/`.prj` sidecar files for Shapefile)
  - Ingested spatial files appear as tables with geometry columns
  - Polygons/lines render on map (not just points)
  - AI analysis can reference geometry columns in SQL
- **Files**: `src-tauri/crates/engine/src/ingest.rs` (extend), `src-tauri/src/lib.rs` (update command), `src/components/FileList.tsx` (accept new file types), `src/components/MapView.tsx` (polygon/line rendering)
- **Dependencies**: None

### TASK-23: Column sort and filter in table preview (est: 3h, role: senior-engineer)
- **Description**: Add column-level sorting (click header to toggle asc/desc) and a row count indicator to the table preview. Phase 2 addition: basic column filter (text search per column).
- **Approach**: Extend the `table_preview` Tauri command to accept optional `order_by` and `filter` parameters. Frontend adds clickable headers and filter input per column.
- **Acceptance criteria**:
  - Clicking a column header sorts by that column (toggle asc → desc → none)
  - Sort state indicated by arrow icon in header
  - Row count indicator shows total rows in table
  - Optional: text filter input per column (WHERE col LIKE '%query%')
- **Files**: `src-tauri/src/lib.rs` (extend preview command), `src/components/FileList.tsx` (sortable headers, filter UI)
- **Dependencies**: None

### TASK-24: Editable SQL panel in chat (est: 3h, role: senior-engineer)
- **Description**: Show the AI-generated SQL in a collapsible panel within each chat response. Allow users to edit and re-execute the SQL. This provides transparency and a power-user escape hatch when AI gets it wrong.
- **Approach**: ChatCard already shows some SQL info. Extend to show full SQL in a collapsible `<pre>` block with an "Edit & Run" button. Edited SQL goes through the existing safety validator before execution.
- **Acceptance criteria**:
  - Each AI response that generated SQL shows a collapsible "View SQL" section
  - SQL is displayed in a monospace, syntax-highlighted text area
  - "Edit" button makes the SQL editable; "Run" button re-executes
  - Edited SQL still passes through the analysis SQL safety validator
  - Results update in the chat message and on the map
- **Files**: `src/components/ChatCard.tsx`, `src/components/SqlEditor.tsx` (new, lightweight)
- **Dependencies**: None

### TASK-25: Example query suggestions in empty chat (est: 2h, role: senior-engineer)
- **Description**: When no conversation is in progress, show clickable example query chips in the ChatCard. Reduces first-use friction by showing users what kinds of questions they can ask.
- **Approach**: Display 4-6 example queries as clickable chips/buttons above the chat input. Clicking one populates the input and submits. Examples should be contextual — if tables are loaded, reference actual column names; if not, show generic examples.
- **Acceptance criteria**:
  - Example chips visible when chat is empty (no messages)
  - Chips disappear after first message is sent
  - At least 4 example queries covering different analysis types (spatial, aggregation, filtering, visualization)
  - If tables are loaded, examples reference actual table/column names
  - Clicking a chip submits the query
- **Files**: `src/components/ChatCard.tsx`, `src/lib/appStore.ts` (table schema for contextual examples)
- **Dependencies**: None

### TASK-26: Increased result limits with pagination (est: 3h, role: senior-engineer)
- **Description**: Increase GeoJSON feature limit to 5,000 and table row limit to 100. Add pagination to the ResultTable for navigating large result sets.
- **Approach**: Update constants in analysis execution. Add OFFSET/LIMIT pagination to the table result query. Frontend adds page navigation controls to ResultTable.
- **Acceptance criteria**:
  - Map renders up to 5,000 GeoJSON features (verify Deck.gl performance)
  - ResultTable shows up to 100 rows per page with next/prev controls
  - Page indicator shows "Page X of Y"
  - Truncation badge (TASK-20) still works with new limits
- **Files**: `src-tauri/crates/engine/src/analysis.rs` (update limits), `src-tauri/src/lib.rs`, `src/components/ChatCard.tsx` (pagination controls)
- **Dependencies**: TASK-20

---

## Phase 3: Insurance Vertical Features (Differentiation)

Goal: Build insurance-specific capabilities that transform Spatia from a generic spatial tool into a purpose-built insurance underwriting intelligence platform. This is the monetization differentiator.

### TASK-27: FEMA flood zone data integration (est: 4h, role: senior-engineer)
- **Description**: Enable loading and querying FEMA National Flood Hazard Layer (NFHL) data. This is the most critical risk overlay for property insurance underwriting.
- **Approach**: FEMA NFHL is available as Shapefile/GeoJSON from FEMA's Map Service Center. Build a Tauri command `load_fema_flood` that downloads or imports FEMA flood zone polygons for a given bounding box into DuckDB via `ST_Read`. Store as a persistent table (`fema_flood_zones`) that the AI can reference in spatial joins.
- **Acceptance criteria**:
  - New command or UI flow to load FEMA flood data for a geographic area
  - Flood zones rendered as semi-transparent polygon overlay on map
  - AI can answer queries like "What percentage of properties are in Zone AE?"
  - Flood zone data persists in DuckDB for reuse
  - Point-in-polygon spatial join works between property table and flood zones
- **Files**: `src-tauri/crates/engine/src/risk_data.rs` (new module), `src-tauri/src/lib.rs`, `src/components/MapView.tsx` (polygon overlay)
- **Dependencies**: TASK-22 (GeoJSON/Shapefile import infrastructure)

### TASK-28: USGS wildfire risk overlay (est: 3h, role: senior-engineer)
- **Description**: Integrate USGS Wildfire Hazard Potential (WHP) data as a risk overlay. WHP provides rasterized wildfire risk scores across the US.
- **Approach**: USGS WHP is available as GeoTIFF raster. Since DuckDB doesn't handle rasters natively, pre-process to vector polygons (risk zones) or use point-sampling. Alternative: use the USGS WHP web service for point-based risk lookups. Store results in DuckDB.
- **Acceptance criteria**:
  - Properties can be scored for wildfire risk (high/moderate/low)
  - Risk scores stored as a column in the property table or as a joined view
  - AI can answer "Which properties have high wildfire risk?"
  - Visual indication on map (color-coded risk)
- **Files**: `src-tauri/crates/engine/src/risk_data.rs`, `src-tauri/src/lib.rs`
- **Dependencies**: TASK-27 (shared risk data infrastructure)

### TASK-29: Insurance-specific AI system prompts (est: 3h, role: senior-engineer)
- **Description**: Replace or augment the generic analysis prompts with insurance-specific system prompts. The AI should understand property insurance terminology, common underwriting questions, risk assessment concepts, and available risk data tables.
- **Approach**: Create insurance-specific prompt templates in `spatia_ai` that inject: (1) insurance domain context (exposure, loss ratio, aggregation, zone classification), (2) available risk data tables (FEMA flood, wildfire), (3) common underwriting query patterns. Use prompt selection based on whether risk data tables are loaded.
- **Acceptance criteria**:
  - When risk data tables exist, AI uses insurance-specific system prompt
  - AI correctly uses insurance terminology in responses
  - AI generates spatial joins between property data and risk overlays without explicit instruction
  - Example queries work: "What's my portfolio exposure in flood Zone AE?", "Flag properties with combined flood and wildfire risk", "Show risk concentration by zip code"
- **Files**: `src-tauri/crates/ai/src/prompts.rs` (new insurance prompts), `src-tauri/crates/ai/src/client.rs` (prompt selection logic)
- **Dependencies**: TASK-27, TASK-28

### TASK-30: Guided risk assessment workflow (est: 5h, role: senior-engineer + ui-design-architect)
- **Description**: Build a step-by-step workflow for the insurance use case: Import Portfolio → Geocode → Load Risk Data → Risk Score → Review → Export Report. This replaces the generic "upload and chat" flow with a task-oriented experience for underwriters.
- **Approach**: New `RiskWorkflow` component that guides users through sequential steps with progress indicators. Each step maps to existing Tauri commands. The workflow is an alternative entry point — the generic chat interface remains available.
- **Acceptance criteria**:
  - Workflow accessible from a prominent UI entry point (toolbar button or welcome screen)
  - Step 1: Import property portfolio (CSV upload)
  - Step 2: Review geocoding results (show confidence, flag low matches)
  - Step 3: Select risk overlays to load (FEMA flood, wildfire)
  - Step 4: View risk assessment summary (property count by risk zone)
  - Step 5: Export results (CSV with risk scores, map PNG)
  - Each step has clear instructions and progress feedback
  - Users can skip steps or return to previous steps
- **Files**: `src/components/RiskWorkflow.tsx` (new), `src/App.tsx` (mount workflow), `src/lib/appStore.ts` (workflow state)
- **Dependencies**: TASK-27, TASK-28, TASK-14, TASK-16

### TASK-31: PDF risk assessment report generation (est: 4h, role: senior-engineer)
- **Description**: Generate a PDF report summarizing the risk assessment results. This is the key deliverable for underwriting workflows — a shareable document that can go into policy files.
- **Approach**: Use a Rust PDF generation library (e.g., `printpdf` or `genpdf`) to create a report containing: map screenshot (from TASK-16), risk summary table, property listing with risk scores, and methodology notes. Triggered from the Risk Workflow or via a "Generate Report" button.
- **Acceptance criteria**:
  - PDF includes: title page, map viewport capture, risk summary statistics, property table with risk scores
  - Generated via native save dialog
  - Professional appearance suitable for inclusion in underwriting files
  - Report data pulled from current analysis state (not re-queried)
- **Files**: `src-tauri/crates/engine/src/report.rs` (new module), `src-tauri/Cargo.toml` (PDF crate), `src-tauri/src/lib.rs` (report command)
- **Dependencies**: TASK-16, TASK-27, TASK-28, TASK-30

### TASK-32: Multi-layer map with user-controlled visibility (est: 4h, role: senior-engineer)
- **Description**: Allow users to toggle visibility of individual map layers (base data, flood zones, wildfire risk, analysis results). Essential for insurance workflows where multiple risk overlays must be compared.
- **Approach**: New `LayerPanel` component listing all active layers with visibility toggles and opacity sliders. Each data source (table points, flood polygons, wildfire zones, analysis overlay) is a separate controllable layer.
- **Acceptance criteria**:
  - Layer panel accessible from map controls (toggle button)
  - Each loaded data source appears as a layer entry
  - Visibility toggle (eye icon) shows/hides the layer
  - Opacity slider per layer
  - Layer ordering (drag to reorder) — stretch goal
  - Panel collapses to not obstruct map view
- **Files**: `src/components/LayerPanel.tsx` (new), `src/components/MapView.tsx` (layer management), `src/lib/appStore.ts` (layer visibility state)
- **Dependencies**: TASK-22, TASK-27

---

## Sprint Status

### MVP Sprint (COMPLETED)

- [x] TASK-P0-1 through TASK-13: All completed (see above)

### Post-MVP Sprint (ACTIVE)

**Phase 1 — Table Stakes (Pre-Launch Blockers):**
- [ ] TASK-14: CSV export of any table
- [ ] TASK-15: GeoJSON export of analysis_result
- [ ] TASK-16: Map PNG export
- [ ] TASK-17: Settings UI — API key management
- [ ] TASK-18: Map legend — auto-generated
- [ ] TASK-19: Basemap selector
- [ ] TASK-20: Truncation indicators
- [ ] TASK-21: Tooltip labels on all controls

**Phase 2 — Competitive Parity:**
- [ ] TASK-22: GeoJSON/Shapefile import
- [ ] TASK-23: Column sort/filter in table preview
- [ ] TASK-24: Editable SQL panel in chat
- [ ] TASK-25: Example query suggestions
- [ ] TASK-26: Increased result limits with pagination

**Phase 3 — Insurance Vertical (Differentiation):**
- [ ] TASK-27: FEMA flood zone data integration
- [ ] TASK-28: USGS wildfire risk overlay
- [ ] TASK-29: Insurance-specific AI prompts
- [ ] TASK-30: Guided risk assessment workflow
- [ ] TASK-31: PDF risk assessment report
- [ ] TASK-32: Multi-layer map with visibility controls

**DEFERRED:**
- [ ] TASK-P0-3: WebDriver E2E test infrastructure

---

## Quality Gate (required before every merge)

```bash
pnpm build
cd src-tauri && cargo test --workspace && cargo clippy --workspace
```

## Team Assignments (Post-MVP)

| Agent | Role | Primary Tasks |
|-------|------|---------------|
| senior-engineer | Full-stack implementation | TASK-14 through TASK-32 (all implementation) |
| test-engineer | Test coverage + acceptance | Tests for each completed task, E2E validation |
| ui-design-architect | UX design | TASK-17 (Settings), TASK-18 (Legend), TASK-30 (Risk Workflow) |
| gis-domain-expert | Domain validation | TASK-27 (FEMA data), TASK-28 (wildfire), TASK-29 (prompts) |
| product-manager | Prioritization + acceptance | Review all deliverables, verify market-fit alignment |
| gis-tech-lead | Architecture + coordination | All tasks (review), dependency management |
