# Spatia Development Plan

## Meta

- [x] Draft and maintain this MVP task list.
      Summary: Added the MVP task list aligned with the current engine/CLI architecture and planned phases.

## Phase 1: The Rust Core (Data Engine)

- [x] Initialize the Rust workspace with Tauri setup.
- [x] Split the backend into `spatia_engine` and `spatia_cli` crates under `src-tauri/crates/`.
      Note: `spatia_cli` is a dev-only tool and is not shipped to end users.
- [x] Add DuckDB and enable the spatial extension during ingestion.
- [x] Add engine-focused dependencies as needed: `serde` (data models), `tokio` + `reqwest` (sidecar/AI calls).
      Summary: Added `serde`, `tokio`, and `reqwest` to the engine crate for upcoming sidecar and AI integration.
- [x] Create a `DbManager` to hold a persistent DuckDB connection (file or in-memory) and reuse it across commands.
      Summary: Added a `DbManager` wrapper with file and in-memory connection helpers.
- [x] Implement `ingest_csv(file_path)` to load raw files into a fixed `raw_staging` table.
      Summary: Added `ingest_csv` to load CSVs into a replaceable `raw_staging` table via DuckDB.
- [x] Extract table schema via `PRAGMA table_info('raw_staging')` and return it from the engine.
      Summary: Added schema helpers returning `TableColumn` metadata from DuckDB PRAGMA queries.
- [x] Add unit tests for ingestion + schema extraction.
      Summary: Added engine tests covering CSV ingestion into raw_staging and schema extraction.
- [x] Ensure `cargo clippy` has zero warnings in the engine crate.
      Summary: Verified the workspace is clippy-clean after engine updates.

## Phase 2: Python Geocoding Sidecar (Removed)

The Python geocoder sidecar (`src-python/spatia-geocoder/`) and all related Rust integration code have been removed. Geocoding is now handled via Overture local search (`overture_geocode`) or the Geocodio API fallback (Phase 2.8).

## Phase 2.5: String-Command Executor (Vercel D1-Style Interface)

- [x] Add `executor.rs` module to engine crate for string-based command parsing.
      Summary: Added parser/tokenizer-driven command dispatch in the engine for shared CLI/Tauri execution.
- [x] Implement `execute_command(command: &str) -> EngineResult<String>` with support for:
  - `ingest <db_path> <csv_path> [table_name]`
  - `schema <db_path> <table_name>`
  - Overture extract/search/geocode commands
    Summary: Added unified JSON-returning command execution for ingest/schema/overture commands.
- [x] Refactor CLI main.rs to use `execute_command()` instead of direct module calls.
      Summary: CLI now serializes argv into command strings and delegates execution to engine executor.
- [x] Add Tauri command `execute_engine_command(command: String)` wrapping the executor.
      Summary: Tauri invoke handler now forwards commands to `spatia_engine::execute_command`.
- [x] Add unit tests for command parsing and execution with various argument formats.
      Summary: Added parser tests for quoted args, optional args, overture commands, and integration round-trip.
- [x] Document the command syntax in CLI help text and architecture.md.
      Summary: Updated CLI help usage/examples and architecture command-surface notes.

## Phase 2.7: Overture + DuckDB Pivot (Map + Search)

- [x] Add Overture release pinning strategy in engine code.
      Summary: Added `OVERTURE_RELEASE` constant and source path construction in `overture.rs`.
- [x] Implement Overture extract command foundation in engine (`overture_extract`).
      Summary: Added bbox parsing, extension loading (`spatial` + `httpfs`), and `CREATE OR REPLACE` extraction from Overture parquet into DuckDB.
- [x] Implement Overture local lookup command foundation in engine (`overture_search`).
      Summary: Added searchable query path against extracted Overture tables with bounded result limit.
- [x] Wire Overture commands through executor and CLI.
      Summary: Added command parsing/dispatch in `executor.rs` and surfaced usage/examples in CLI help.
- [x] Add normalized lookup schema (name tokens/ranking fields) for stable geocoding-like relevance.
      Summary: Added `{table}_lookup` normalized table generation and ranked search ordering for more stable lookup relevance.
- [x] Add external precompute workflow for PMTiles build from DuckDB extracts.
      Summary: Added executable script `src-tauri/scripts/build_overture_pmtiles.sh` and documented CLI workflow in README.
- [x] Add end-to-end acceptance check: `overture_extract -> overture_search` on a sample bbox.
      Summary: Verified command flow against Seattle bbox with release `2026-02-18.0` and confirmed extraction + local search output.

## Phase 2.8: Geocodio API Backup Geocoding with Intensive Caching

- [ ] Add `geocodio` module to `spatia_engine` crate with a `geocode_via_geocodio(addresses)` function that calls the Geocodio REST API using `reqwest`.
      Notes: Requires `SPATIA_GEOCODIO_API_KEY` env var. Endpoint: `https://api.geocodio.com/v1.7/geocode?api_key=<key>` (batch POST, up to 10 000 addresses per request).
- [ ] Create a DuckDB-backed geocoding cache table (`geocode_cache`) with columns: `address TEXT PRIMARY KEY, lat REAL, lon REAL, source TEXT, cached_at TIMESTAMP`.
      Notes: Cache is stored in the app's DuckDB file so results persist across sessions; `source` records the provider (e.g. `geocodio` or `overture`).
- [ ] Implement cache-read helper `cache_lookup(conn, addresses) -> (hits, misses)` to split an address batch into already-cached results and uncached ones.
- [ ] Implement cache-write helper `cache_store(conn, results, source)` that upserts resolved results into `geocode_cache` using `INSERT OR REPLACE`.
- [ ] Add a `geocode` command to the executor that uses cache → Geocodio fallback → write cache.
- [ ] Add unit tests for cache lookup/store helpers and the Geocodio fallback branch (mock HTTP with a fixture response).
- [ ] Document new env vars (`SPATIA_GEOCODIO_API_KEY`, `SPATIA_GEOCODIO_BATCH_SIZE`) in CLI help text and `architecture.md`.
- [ ] Ensure `cargo clippy` has zero warnings after integration.

## Phase 3: The AI Brain (Data Cleaner)

- [ ] Add a Gemini client (SDK or REST via `reqwest`) behind a feature flag or config.
- [ ] Fetch 20 random rows from `raw_staging` and the schema for context.
- [ ] Define a prompt that requests DuckDB `UPDATE` statements to clean data.
- [ ] Execute the generated SQL and validate column types after cleanup.

## Phase 4: The Interactive Frontend (UI)

- [ ] Initialize TanStack Router with `/upload` and `/map` routes.
- [ ] Build `/upload` with file picker and extraction progress events from the Rust backend.
- [ ] Build `/map` using MapLibre GL JS with local PMTiles vector sources generated from Overture extracts.
- [ ] Add base layer style + source wiring for places/names, roads, buildings, and boundaries.
- [ ] Add map layer toggles and attribution panel showing Overture release/source metadata.
- [ ] Connect map search UI to `overture_search` results and pan/zoom to selected feature.

## Phase 5: The Analysis Loop (Golden Path)

- [ ] Inject the current DuckDB schema into the AI system prompt on user chat.
- [ ] Ask the AI to generate a DuckDB SQL query that creates an `analysis_result` view.
- [ ] Execute the SQL, return GeoJSON to the frontend, and render on the map.
- [ ] Request a structured visualization command from the AI (e.g., `{ "visualization": "scatter" }`).
- [ ] Update the React UI to parse the command and update Deck.gl layers dynamically.
