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

## Quality Gate (required before every merge)

```bash
pnpm build
cd src-tauri && cargo test --workspace && cargo clippy --workspace
```

## Agent Testing Capabilities

| Agent | Can Do | Tools |
|-------|--------|-------|
| test-engineer | Write and run E2E tests, assert on UI state, take screenshots | WebDriverIO, `debug_ui_snapshot`, `capture-app.sh` |
| product-manager | Visually verify features, read UI state, acceptance testing | `capture-app.sh` (Read PNG), `dump-ui-state.sh` (Read JSON) |
| ui-design-architect | Verify layout/styling, check component rendering | `capture-app.sh` (Read PNG), `dump-ui-state.sh` |
| senior-engineer | Run full test suite, debug failures, add E2E tests | All tools |

## Team Assignments

| Agent | Role | Primary Tasks |
|-------|------|---------------|
| senior-engineer | Full-stack implementation | TASK-P0-1/2/3, TASK-01 through TASK-10, TASK-13 |
| test-engineer | Test coverage + E2E testing | TASK-11, ongoing E2E test writing after P0 |
| ui-design-architect | UX design + visual verification | TASK-12 |
| gis-domain-expert | Consult on geocoding, CRS, spatial queries | Advisory on TASK-02, TASK-10 |
| product-manager | Prioritization, acceptance, visual verification | Review all task completions |
| gis-tech-lead | Architecture, code review, coordination | TASK-P0-4, all tasks (review) |
