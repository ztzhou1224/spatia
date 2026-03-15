# Senior Engineer — Project Memory

## Key File Locations

| Path | Purpose |
|------|---------|
| `src-tauri/src/lib.rs` | All Tauri commands; register new commands in `invoke_handler` |
| `src-tauri/crates/engine/src/lib.rs` | Engine public API re-exports |
| `src-tauri/crates/engine/src/identifiers.rs` | `validate_table_name` — use for any user SQL input |
| `src-tauri/crates/engine/src/analysis.rs` | `execute_analysis_sql_to_geojson` |
| `src-tauri/crates/engine/src/geocode.rs` | Batch geocoding; local Overture + Geocodio fallback |
| `src-tauri/crates/ai/src/client.rs` | Gemini HTTP client (feature-gated `gemini`) |
| `src/lib/appStore.ts` | Zustand store — tables, chatMessages, analysisGeoJson, mapActions, logPath |
| `src/lib/mapActions.ts` | Map imperative actions (via MapViewHandle ref) |
| `src/lib/tauri.ts` | `isTauri()` guard; import `invoke` from `@tauri-apps/api/core` |
| `src/components/MapView.tsx` | Map component; exposes `MapViewHandle` ref |
| `src/components/ChatCard.tsx` | AI chat UI; reads from appStore |
| `src/components/FileList.tsx` | Uploaded files list |
| `src/lib/debug.ts` | DEV-only `installDebugSnapshot()` — call from main.tsx; exposes `window.__spatia_debug_snapshot()` |
| `scripts/dump-ui-state.sh` | Shell script that triggers snapshot via osascript JXA and reads `scripts/screenshots/ui-state.json` |

## Architecture Patterns

- **Tauri command pattern**: `#[tauri::command] fn foo(...) -> Result<T, String>` — errors must be `String` at the boundary.
- **Engine errors**: Use `EngineResult<T>` = `Result<T, Box<dyn std::error::Error>>` internally; convert with `.map_err(|e| e.to_string())` at the Tauri boundary.
- **DuckDB extensions**: Load `spatial` and `httpfs` on every new connection (connection-scoped, not persistent).
- **Analysis SQL enforcement**: `execute_analysis_sql` in engine validates the strict prefix `CREATE [OR REPLACE] VIEW analysis_result AS` before running.
- **Widget store replaced**: The old `widgetStore.ts`/`aiContext.ts`/`useFocusGuard.ts` were deleted. State now consolidates in `appStore.ts`.
- **Bottom-up implementation**: For cross-layer tasks, implement engine first → Tauri command → frontend.

## Tracing / Logging Infrastructure

- Initialized in `run()` in `src-tauri/src/lib.rs` via `tracing-subscriber` with two layers: rolling daily file (`logs/spatia.log`) and stderr.
- `LOG_PATH` static stores the log file path; exposed via `get_log_path` Tauri command (registered in both debug and release handler lists).
- `tracing` crate added to `spatia_engine` and `spatia_ai` crates; `tracing-subscriber` and `tracing-appender` only in the root app crate.
- Default level: `info`; override with `RUST_LOG=debug` env var.
- `logs/` directory is already in `.gitignore` (line 2: `logs`).
- `appStore.ts` has `logPath` field and `fetchLogPath()` action; called in `App.tsx` alongside `fetchApiConfig`.
- `ChatCard.tsx` and `FileList.tsx` display `logPath` hint below error messages.

## Debug Infrastructure

- `write_debug_snapshot` Tauri command is compiled only under `#[cfg(debug_assertions)]`; the invoke_handler in `lib.rs` uses a `{ #[cfg(...)] { generate_handler![...] } }` block pattern to select at compile time.
- `installDebugSnapshot()` in `debug.ts` is gated behind `import.meta.env.DEV`; must be called from `main.tsx` before `ReactDOM.createRoot`.
- Snapshot writes to `scripts/screenshots/ui-state.json` (relative to process cwd; `write_debug_snapshot` tries `<cwd>/scripts/screenshots` then `<cwd>/../scripts/screenshots`).

## SQL Injection Prevention in preview_table

- **Column names**: Cannot be parameterized. Validate against `col_names` from schema (whitelist check: `col_names.iter().any(|c| c == input)`).
- **Table names**: Validate via `spatia_engine::validate_table_name` (allows `[a-zA-Z_][a-zA-Z0-9_]*` only).
- **Filter values**: Escape single-quotes with `fv.replace('\'', "''")` then interpolate into `ILIKE '%value%'`. ILIKE handles wildcards safely when the delimiters are literal `%`.
- **Sort direction**: Whitelist — only accept `"desc"` → `DESC`, default anything else to `ASC`.

## Gotchas

- `PRAGMA table_info` boolean columns deserialize as Rust `bool`, not the string `"BOOLEAN"`.
- Temp test DuckDB files leave `.wal` and `.wal.lck` alongside `.duckdb` — all three must be cleaned up.
- `SPATIA_GEMINI_API_KEY` env var must be set at runtime for any AI path; missing key causes a runtime error, not a compile error.
- `identifiers::validate_table_name` only allows `[a-zA-Z_][a-zA-Z0-9_]*` — reject table names with hyphens or dots.
- Overture release is pinned via `SPATIA_OVERTURE_RELEASE` env var or a default constant in engine; changing it affects reproducibility of extracts.
- **duckdb-rs 1.4.4 `column_count()` panics before execute**: `Statement::column_count()` calls `result_unwrap()` which panics with "The statement was not executed yet" if called before `stmt.query()` or `stmt.execute()`. Always call `query()` first, then get column count via `rows.as_ref().map(|s| s.column_count()).unwrap_or(0)`. Fixed in `src-tauri/crates/ai/src/cleaner.rs` `fetch_sample_rows`.

## See Also

- `patterns.md` — detailed patterns for recurring implementation tasks (to be created as needed)
