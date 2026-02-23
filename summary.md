# Spatia Summary (Compact)

## Purpose

Fast memory file for implementation constraints, recurring pitfalls, and daily commands.

## Current Stack

- Frontend: React + TypeScript + Vite
- Desktop shell: Tauri v2
- Core data engine: Rust (`src-tauri/crates/engine`)
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
4. If a `.duckdb` temp file is used in tests, clean `.duckdb`, `.wal`, and `.wal.lck`.

## Stable Engine Patterns

- Shared result type: `EngineResult<T> = Result<T, Box<dyn Error + Send + Sync>>`
- Module boundaries:
  - `db_manager`: DuckDB connection lifecycle
  - `ingest`: CSV import + extension load
  - `schema`: schema introspection
  - `identifiers`: SQL-safe validation helpers
  - `geocode`: Geocodio API backup geocoding with DuckDB `geocode_cache` persistence
  - `types`: shared aliases/types
- Prefer root-level re-exports in `lib.rs` for public API consistency.

## Operational Commands

- Rust tests: `cargo test --workspace`
- Rust lint: `cargo clippy --workspace`
- Tauri dev: `pnpm tauri dev`
- Tauri build: `pnpm tauri build`

## Key Paths

- Engine crate: `src-tauri/crates/engine/src`
- CLI crate: `src-tauri/crates/cli/src`
- Tauri app: `src-tauri/src`
- Tauri config: `src-tauri/tauri.conf.json`

## Active Risks (Short)

- Overture release/schema drift without strict pinning
- Long-running operations can affect UX if not async/backgrounded
- SQL string construction requires careful escaping and validation

## Rule of Thumb

If context needs to be loaded quickly, prefer this file + `plan.md` + `context.md`; keep deep rationale in `architecture.md` minimal and current.
