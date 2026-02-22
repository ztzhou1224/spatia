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

## Phase 2: The Python Geocoding Sidecar

- [x] Initialize a new Python project at `src-python/spatia-geocoder`.
      Summary: Added the `spatia-geocoder` scaffold with a minimal `pyproject.toml` and entrypoint.
- [x] Install Python dependencies: `fastapi`, `uvicorn`, `geopy`, `pyinstaller`.
      Summary: Added geocoder dependencies to `pyproject.toml` and installed them in the workspace venv.
- [x] Implement a FastAPI `POST /geocode` endpoint that accepts a list of addresses and returns coordinates.
      Summary: Added a FastAPI geocode endpoint backed by geopy with Pydantic request/response models.
- [x] Compile the Python app with `pyinstaller --onefile main.py`.
      Summary: Built a single-file geocoder binary using PyInstaller.
- [x] Add a script to rename the binary with the host target triple and move it to `src-tauri/binaries/`.
      Summary: Added a packaging script to copy the PyInstaller binary into `src-tauri/binaries` with a host triple name.
- [x] Update [src-tauri/tauri.conf.json](src-tauri/tauri.conf.json) to include `externalBin` for the sidecar.
      Summary: Added `binaries/spatia-geocoder` to the Tauri bundle external binaries list.
- [x] Update [src-tauri/src/main.rs](src-tauri/src/main.rs) to spawn the sidecar using `tauri_plugin_shell`.
      Summary: Added `tauri-plugin-shell` and spawn logic in [src-tauri/src/lib.rs](src-tauri/src/lib.rs).
- [x] Add `geocode_batch(address_list)` in Rust using `reqwest` to call the local sidecar.
      Summary: Added `geocode_batch` in the engine crate with reqwest-based HTTP client to call the local geocoder sidecar.

## Phase 2.5: String-Command Executor (Vercel D1-Style Interface)

- [x] Add `executor.rs` module to engine crate for string-based command parsing.
      Summary: Added parser/tokenizer-driven command dispatch in the engine for shared CLI/Tauri execution.
- [x] Implement `execute_command(command: &str) -> EngineResult<String>` with support for:
  - `ingest <db_path> <csv_path> [table_name]`
  - `schema <db_path> <table_name>`
  - `geocode <address_1> <address_2> ...`
    Summary: Added unified JSON-returning command execution for ingest/schema/geocode.
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
      Notes: Cache is stored in the app's DuckDB file so results persist across sessions; `source` records whether the result came from `sidecar` or `geocodio`.
- [ ] Implement cache-read helper `cache_lookup(conn, addresses) -> (hits, misses)` to split an address batch into already-cached results and uncached ones.
- [ ] Implement cache-write helper `cache_store(conn, results, source)` that upserts resolved results into `geocode_cache` using `INSERT OR REPLACE`.
- [ ] Integrate Geocodio as the fallback in `geocode_batch_hybrid`: after the sidecar path fails or returns partial results, call `geocode_via_geocodio` for the remaining addresses, then write all results to the cache.
- [ ] Wrap the full geocode call path in a cache-first pattern: check cache → sidecar → Geocodio fallback → write cache.
- [ ] Add unit tests for cache lookup/store helpers and the Geocodio fallback branch (mock HTTP with a fixture response).
- [ ] Update `executor.rs` so the `geocode` command passes the active DuckDB connection to `geocode_batch_hybrid` for cache access.
- [ ] Document new env vars (`SPATIA_GEOCODIO_API_KEY`, `SPATIA_GEOCODIO_BATCH_SIZE`) in CLI help text and `architecture.md`.
- [ ] Ensure `cargo clippy` has zero warnings after integration.
      Summary: Add Geocodio API backup geocoding path, intensive DuckDB-backed result cache, and cache-first dispatch in the hybrid geocoder.

## Phase 2.9: MCP Server (AI Tool Integration)

- [x] Research Model Context Protocol (JSON-RPC 2.0 over stdio, `initialize` / `tools/list` / `tools/call` methods).
      Summary: MCP uses newline-delimited JSON-RPC 2.0; tools expose JSON Schema inputs; error codes -32700/-32601/-32602/-32603.
- [x] Create `src-tauri/crates/mcp/` crate (`spatia_mcp`) with a `spatia-mcp` binary.
      Summary: Single-file implementation in `src/main.rs`; no new external dependencies (reuses `serde`/`serde_json`/`spatia_engine`).
- [x] Implement MCP protocol handler: `initialize`, `ping`, `tools/list`, `tools/call`, notification pass-through.
      Summary: All four methods implemented; notifications (no `id`) silently ignored; proper JSON-RPC error codes returned.
- [x] Expose all six engine commands as MCP tools: `ingest_csv`, `get_schema`, `geocode`, `overture_extract`, `overture_search`, `overture_geocode`.
      Summary: Each tool maps arguments to the engine string-command format via `build_command`; tool execution errors returned as `isError: true` content.
- [x] Add unit tests covering `build_command` for every tool and `handle_line` for every MCP method.
      Summary: 18 tests all pass; zero clippy warnings.
- [x] Update `architecture.md` with MCP server section.
      Summary: Added MCP layer description, tool table, protocol methods, and Claude Desktop config example.
- [x] Update `plan.md` and `summary.md`.
      Summary: Recorded new crate path, binary name, and quickstart command.



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
