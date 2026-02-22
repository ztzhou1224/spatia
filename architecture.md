# Spatia Architecture (Compact)

## System Shape

Spatia is a desktop GIS app with four layers:

1. React/Vite frontend (UI)
2. Tauri host (desktop runtime + command bridge)
3. Rust engine crate (data + spatial operations)
4. Overture data pipeline (DuckDB extraction + PMTiles outputs)

## Core Decisions

### 1) Rust Multi-Crate Workspace

- `spatia` (Tauri app shell)
- `spatia_engine` (reusable domain logic)
- `spatia_cli` (CLI wrapper over engine)

Why: isolates business logic, improves testability, keeps GUI/CLI thin.

### 2) DuckDB + Spatial Extension

- Embedded DB, file-based, no external server.
- Spatial extension is required for GIS functions and loaded at runtime.

Why: strong analytical SQL, portable DB files, low deployment complexity.

### 3) Overture + DuckDB First

- Overture GeoParquet is queried via DuckDB with bbox filters.
- Derived tables power map layers and search/geocoding-like lookups.
- PMTiles artifacts are built from extracted data for frontend rendering.

Why: unify map + lookup data source, reduce runtime complexity, and improve offline caching paths.

### 4) Unified Engine Error Surface

- Engine APIs return `EngineResult<T>` with thread-safe boxed errors.

Why: reduces boilerplate and keeps async boundaries ergonomic.

### 5) Strict Input Validation

- SQL identifiers validated before use.
- File paths escaped when embedded in SQL strings.

Why: prevent SQL injection/syntax breakage with user-provided inputs.

### 6) Transitional Compatibility

- Existing Python sidecar geocoding remains temporarily available during migration.
- New roadmap work prioritizes Overture commands and sidecar deprecation.

## Data Flows

### CSV Ingestion

Frontend path -> Tauri command -> engine opens DuckDB -> loads spatial extension -> `read_csv_auto` into staging table -> schema returned to UI.

### Overture Extract + Search

Engine queries Overture parquet -> stores bounded local tables in DuckDB -> exports/builds PMTiles -> frontend renders vector layers and runs lookup against local tables.

### CLI

CLI parses command -> calls engine functions -> prints structured output/errors.

## String Command Syntax

- `ingest <db_path> <csv_path> [table_name]`
- `schema <db_path> <table_name>`
- `geocode <address_1> <address_2> ...` (legacy, transitional)
- Overture extract/search commands (planned replacement)

Notes:

- Quoted addresses are supported for geocoding (example: `"San Francisco, CA"`).
- Engine returns JSON strings so CLI and Tauri share one command surface.

## Repository Map

- Frontend: `src/`
- Tauri host: `src-tauri/src/`
- Engine: `src-tauri/crates/engine/src/`
- CLI: `src-tauri/crates/cli/src/`
- Overture extraction scripts and outputs: `src-tauri/` + data artifacts (planned)

## Quality Gates

- `cargo test --workspace`
- `cargo clippy --workspace`

## Known Constraints

- Spatial extension availability is connection-scoped.
- Overture release/version pinning is required for reproducible outputs.

## Next Evolution (Shortlist)

- Overture extract commands + PMTiles precompute workflow
- MapLibre PMTiles rendering baseline with layer toggles
- Sidecar command deprecation once Overture search parity is reached
