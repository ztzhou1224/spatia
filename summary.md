# Spatia Summary (Compact)

## Purpose

Fast memory file for implementation constraints, recurring pitfalls, and daily commands.

## Current Stack

- Frontend: React + TypeScript + Vite
- Desktop shell: Tauri v2
- Core data engine: Rust (`src-tauri/crates/engine`)
- MCP server: Rust (`src-tauri/crates/mcp`) – JSON-RPC 2.0 stdio tool server
- Data source direction: Overture + DuckDB (in migration)
- Database: DuckDB + spatial extension

## Non-Negotiables

- Rust quality gate: `cargo test` and `cargo clippy` must pass before finalizing changes.
- Keep engine memory-safe and warning-free.
- Do not rewrite core architecture or DB schema without explicit permission.
- Validate SQL identifiers for any user-provided table/column names.

## High-Value Gotchas

1. DuckDB `PRAGMA table_info` boolean fields are `bool`, not `i64`.
2. Spatial extension must be loaded per connection before spatial SQL.
3. Overture extraction must pin release/version for reproducible results.
4. If sidecar is used during migration, binary naming must include target triple suffix.
5. CSV paths in SQL need single-quote escaping (`'` -> `''`).
6. Tests touching DuckDB temp files should clean `.duckdb`, `.wal`, and `.wal.lck`.

## Stable Engine Patterns

- Shared result type: `EngineResult<T> = Result<T, Box<dyn Error + Send + Sync>>`
- Module boundaries:
  - `db_manager`: DuckDB connection lifecycle
  - `ingest`: CSV import + extension load
  - `schema`: schema introspection
  - `identifiers`: SQL-safe validation helpers
  - `geocode`: transitional legacy geocode path
  - `types`: shared aliases/types
- Prefer root-level re-exports in `lib.rs` for public API consistency.

## Operational Commands

- Rust tests: `cargo test --workspace`
- Rust lint: `cargo clippy --workspace`
- Tauri dev: `pnpm tauri dev`
- Tauri build: `pnpm tauri build`
- MCP server: `cargo run -p spatia_mcp` (reads JSON-RPC from stdin, writes to stdout)
- Legacy sidecar local run: `python src-python/spatia-geocoder/main.py`
- Legacy sidecar package: `bash src-python/spatia-geocoder/scripts/package_sidecar.sh`

## Key Paths

- Engine crate: `src-tauri/crates/engine/src`
- CLI crate: `src-tauri/crates/cli/src`
- MCP server crate: `src-tauri/crates/mcp/src`
- Tauri app: `src-tauri/src`
- Sidecar app: `src-python/spatia-geocoder/main.py`
- Tauri config: `src-tauri/tauri.conf.json`

## Active Risks (Short)

- Overture release/schema drift without strict pinning
- Long-running operations can affect UX if not async/backgrounded
- SQL string construction requires careful escaping and validation

## Rule of Thumb

If context needs to be loaded quickly, prefer this file + `plan.md` + `context.md`; keep deep rationale in `architecture.md` minimal and current.
