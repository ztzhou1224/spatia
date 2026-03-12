# Test Engineer Memory — Spatia

## Key File Locations
- Analysis tests: `src-tauri/crates/engine/src/analysis.rs` (#[cfg(test)] module at bottom)
- Geocode unit tests: `src-tauri/crates/engine/src/geocode.rs` (#[cfg(test)] module)
- Geocode integration tests: `src-tauri/crates/engine/src/geocode_integration_tests.rs` (untracked file, pre-existing)
- Executor tests: `src-tauri/crates/engine/src/executor.rs` (#[cfg(test)] module)

## Known Pre-existing Failures
- `geocode_integration_tests::tests::end_to_end_ingest_and_geocode_with_seeded_cache` — fails because `data/test_geocode_addresses.csv` does not exist. The file `geocode_integration_tests.rs` is untracked and that CSV is missing from `data/`.

## DuckDB Temp File Patterns
- Path pattern: `/tmp/spatia_<module>_test_<nanos>.duckdb`
- Always clean `.duckdb`, `.duckdb.wal`, `.duckdb.wal.lck`
- Drop connections before calling functions that open their own connections
- In-memory `Connection::open_in_memory()` is fine for unit tests that don't need file-based isolation

## Analysis Module Details
- `TabularResult` has `columns: Vec<String>`, `rows: Vec<Vec<Value>>`, `truncated: bool`
- `TABULAR_ROW_LIMIT = 20` — tabular fetches limit+1, truncates to 20, sets `truncated: true`
- GeoJSON fetches up to 1000 rows (separate pass from tabular)
- All column values are CAST to VARCHAR before extraction — everything arrives as `Value::String`
- Coordinate lookup checks: lat=["lat","latitude","_lat"], lon=["lon","lng","longitude","_lon"] (case-insensitive)
- Non-spatial rows get `"geometry": null` in the GeoJSON feature (features are still present, geometry is JSON null)
- `row_count` equals the number of GeoJSON features returned (includes null-geometry features)
- `validate_analysis_sql` uses blocklist regexes with `\b` word boundaries — column names containing blocked words as substrings are NOT flagged

## validate_analysis_sql Rules
- Must start with `CREATE [OR REPLACE] VIEW analysis_result AS` (case-insensitive after trim)
- Body is scanned for blocklist patterns: DROP TABLE/VIEW/SCHEMA/DATABASE, TRUNCATE, DELETE FROM, ALTER TABLE/VIEW, GRANT, REVOKE, INSERT INTO, UPDATE, COPY, ATTACH, DETACH
- Regexes use `\b` so `drop_count`, `update_time`, etc. are allowed

## Test Running Commands
- Module-level: `cargo test -p spatia_engine --lib analysis`
- Full workspace: `cargo test --workspace` (from `src-tauri/`)
- Clippy: `cargo clippy --workspace` (from `src-tauri/`)

## Details: See
- `patterns.md` for setup/teardown patterns
