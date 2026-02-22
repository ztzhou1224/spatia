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

### 6) Geocodio API Backup Geocoding with Intensive Caching

When the local Python sidecar is unavailable or returns incomplete results, the engine falls back to the [Geocodio](https://www.geocodio.com/) REST API for batch geocoding.  To minimise paid API calls, every resolved address is immediately persisted in a DuckDB table (`geocode_cache`) so it is served from cache on all subsequent requests.

**Dispatch order** (cache-first):
1. Check `geocode_cache` in DuckDB — return cached coordinates for matching addresses.
2. Send remaining addresses to the local Python sidecar (geopy).
3. For any addresses the sidecar cannot resolve, send them to the Geocodio batch endpoint.
4. Write all newly resolved results to `geocode_cache` (upsert on `address`).

**Cache schema:**
```sql
CREATE TABLE IF NOT EXISTS geocode_cache (
    address   TEXT PRIMARY KEY,
    lat       REAL NOT NULL,
    lon       REAL NOT NULL,
    source    TEXT NOT NULL,   -- 'sidecar' | 'geocodio'
    cached_at TIMESTAMP DEFAULT current_timestamp
);
```

**Configuration env vars:**
- `SPATIA_GEOCODIO_API_KEY` — required to enable the Geocodio fallback.
- `SPATIA_GEOCODIO_BATCH_SIZE` — max addresses per Geocodio request (default 100, max 10 000).

Why: eliminates redundant external calls, keeps costs predictable, and provides a reliable path when the sidecar binary is absent (e.g., CI or fresh installs).

### 7) Transitional Compatibility

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

- Geocodio API backup geocoding module + DuckDB `geocode_cache` (Phase 2.8)
- Overture extract commands + PMTiles precompute workflow
- MapLibre PMTiles rendering baseline with layer toggles
- Sidecar command deprecation once Overture search parity is reached
