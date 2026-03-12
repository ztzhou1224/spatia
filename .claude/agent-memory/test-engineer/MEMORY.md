# Test Engineer Memory

## Temp DB Pattern (confirmed from engine tests)

Use nanosecond timestamp, NOT tempfile crate or UUID:
```rust
fn tmp_db_path(module: &str) -> String {
    let ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("/tmp/spatia_{module}_test_{ns}.duckdb")
}
fn cleanup_db(path: &str) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}.wal"));
    let _ = std::fs::remove_file(format!("{path}.wal.lck"));
}
```

## Integration Test File Convention

NOT `tests/` directory. Use a sibling file `src/<name>_integration_tests.rs` and add:
```rust
// in lib.rs:
#[cfg(test)]
mod geocode_integration_tests;
```

## Test Infrastructure Available

- `mockito` crate for HTTP mocking (Geocodio); use `SPATIA_GEOCODIO_BASE_URL` env var
- `#[tokio::test]` for async tests
- `Connection::open_in_memory()` for pure logic tests without file I/O
- CSV fixtures in `data/` dir, referenced via `env!("CARGO_MANIFEST_DIR")`

## Coverage Gaps (as of 2026-03-09)

- `identifiers.rs` — `validate_table_name` has NO tests (SQL injection risk)
- `schema.rs` — `table_schema` and `raw_staging_schema` have NO tests
- `spatia_ai` crate — prompt builder tests are pure and missing
- `overture.rs` — hard to unit test (S3 I/O); consider mock-based or skip with `#[ignore]`

## Key Test Locations

- `src-tauri/crates/engine/src/analysis.rs` — analysis SQL prefix validation tests
- `src-tauri/crates/engine/src/geocode.rs` — geocode cache unit tests
- `src-tauri/crates/engine/src/geocode_integration_tests.rs` — end-to-end geocode batch tests
- `src-tauri/crates/engine/src/ingest.rs` — CSV ingest + schema roundtrip tests

## Memory Path

`/Users/zhaotingzhou/Projects/spatia/.claude/agent-memory/test-engineer/`
(NOT under src-tauri — that path is wrong)
