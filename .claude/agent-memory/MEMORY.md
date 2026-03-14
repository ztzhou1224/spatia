# Spatia Agent Memory

## Key File Locations
- Engine crate: `src-tauri/crates/engine/src/` — `overture.rs`, `lib.rs`, `analysis.rs`, `ingest.rs`, `schema.rs`
- Tauri command surface: `src-tauri/src/lib.rs` — all `#[tauri::command]` fns, two `generate_handler!` blocks (debug + release)
- Overture S3 path pattern: `s3://overturemaps-us-west-2/release/{release}/theme={theme}/type={type}/*`
- Default Overture release constant: `overture::OVERTURE_RELEASE = "2026-02-18.0"` (override via `SPATIA_OVERTURE_RELEASE`)

## Patterns
- `EngineResult<T>` = `Result<T, Box<dyn std::error::Error>>` — use for all fallible engine fns
- `ensure_extensions(&conn)` loads `spatial` + `httpfs` extensions — call before any spatial/S3 ops
- All new Tauri commands must be registered in BOTH `generate_handler!` blocks (debug + release) in `src-tauri/src/lib.rs`
- Public engine API is re-exported from `src-tauri/crates/engine/src/lib.rs` — add `pub use` there
- `BBox::parse("xmin,ymin,xmax,ymax")` is the standard bbox parsing utility in `overture.rs`
- DuckDB params use `duckdb::params![...]` macro for parameterized queries

## Environment Constraints
- GTK/gdk-3.0 system libs not installed in sandbox — `cargo check --workspace` will fail on `spatia` (Tauri) crate; use `cargo check -p spatia_engine` instead
- `cargo check -p spatia_engine -p spatia_ai -p spatia_cli` is the reliable non-GTK check command
- `rustfmt --edition 2021 --check` is needed (not default 2015) to check formatting correctly
