# Codebase Patterns (Spatia) - Updated 2026-03-09

## Tauri Command Pattern
All commands in `src-tauri/src/lib.rs`:
- Sync commands: `fn name(args) -> Result<String, String>`
- Async commands: `async fn name(app: tauri::AppHandle, args) -> Result<String, String>`
- Return JSON strings (not typed), parsed on frontend with `JSON.parse(raw)`
- Progress via Tauri events: `app.emit("event-name", payload)`
- DB access via `db_path()` which reads from OnceLock (set in setup hook to app-data dir)

## Engine Command Flow
CLI: `spatia_cli` -> `executor::execute_command(&str)` -> parse tokens -> dispatch to module fns
Tauri: lib.rs calls engine fns directly (bypasses executor for most commands)

## DuckDB Connection Pattern
- Each operation opens its own connection: `Connection::open(db_path)`
- No connection pooling (DuckDB file locks handled by DuckDB itself)
- Extensions loaded per-connection: `INSTALL spatial; LOAD spatial;`
- geocode_batch opens/closes connection internally (avoid holding locks during async work)

## Frontend State (Zustand)
- Single store `useAppStore` in `src/lib/appStore.ts`
- Tables: TableInfo[] with status machine (ingesting -> cleaning -> detecting -> ready -> geocoding -> done)
- Chat: ChatMessage[] with role/content/sql/rowCount
- Analysis: analysisGeoJson (unknown type, set from chat_turn response)
- MapActions: handled imperatively via executeMapActions()

## AI Prompt Pattern
- Schema injected into prompts via `build_*_prompt()` functions
- Unified chat uses `build_unified_chat_prompt()` with multi-table schemas + conversation history
- JSON response mode via `generate_json()` (sets response_mime_type)
- Cleaner uses plain text mode via `generate()` (returns SQL statements line by line)

## Map Rendering
- MapLibre as base map (OSM raster tiles)
- PMTiles sources for vector overlays (places, names, roads, buildings, boundaries)
- Analysis results: GeoJSON source with circle/fill/line layers
- Deck.gl overlay: ScatterplotLayer (interleaved=false)
- Auto-fitBounds on analysis results

## SQL Safety
- `validate_table_name()`: alphanumeric + underscore, starts with letter/underscore
- `validate_analysis_sql()`: prefix check only (CREATE [OR REPLACE] VIEW analysis_result AS)
- Cleaner `validate_statement()`: UPDATE-only allowlist
- Address column in geocode_table_column: reject empty or containing double-quotes
- Schema queries use parameterized WHERE or single-quoted escaping

## File Naming Convention
- Rust: snake_case modules, pub fn exports in lib.rs
- Frontend: PascalCase components, camelCase utilities
- CSS: BEM-like classes (chat-card, chat-card--expanded)
