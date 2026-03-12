# Agent Memory

## Key File Locations
- Engine geocoding: `src-tauri/crates/engine/src/geocode.rs` — `geocode_batch`, `cache_lookup`, `local_fuzzy_geocode`, `GeocodeStats`
- Engine lib exports: `src-tauri/crates/engine/src/lib.rs` — pub use statements for all public types
- Tauri commands: `src-tauri/src/lib.rs` — all #[tauri::command] functions; `run_geocode_for_column` is a sync helper used by both `geocode_table_column` and `ingest_file_pipeline`
- Frontend store: `src/lib/appStore.ts` — Zustand store, `TableInfo` type
- Upload UI: `src/components/FileList.tsx` — pipeline invocation, geocode trigger, table card rendering
- Integration tests: `src-tauri/crates/engine/src/geocode_integration_tests.rs` (untracked file)

## Key Patterns
- `geocode_batch` returns `(Vec<GeocodeBatchResult>, GeocodeStats)` — destructure with `let (results, stats) = ...`
- `run_geocode_for_column` returns `Result<(usize, GeocodeStats), String>`
- Pipeline JSON includes `geocode_stats: { total, geocoded, by_source: { cache, overture_fuzzy, geocodio }, unresolved }`
- `geocode_table_column` JSON includes `by_source` and `unresolved` at top level
- Pre-existing test failure: `end_to_end_ingest_and_geocode_with_seeded_cache` — missing `data/test_geocode_addresses.csv`
- DuckDB connections are connection-scoped for extensions; each connection needs `LOAD spatial`
- Analysis SQL must start with `CREATE [OR REPLACE] VIEW analysis_result AS`
- All SQL identifiers validated via `identifiers::validate_table_name`
