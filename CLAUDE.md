# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What Is Spatia

Spatia is a desktop GIS app (Tauri + React + Rust/DuckDB) supporting CSV ingestion, Overture-backed local geocoding, PMTiles map rendering, and schema-aware AI analysis via a map-centric chat interface.

## Commands

### Development

```bash
pnpm tauri dev          # Run desktop app with hot-reload
pnpm build              # TypeScript + Vite frontend build
```

### Rust (run from `src-tauri/`)

```bash
cargo test --workspace
cargo clippy --workspace
cargo build --workspace
cargo run -p spatia_cli -- ingest ./spatia.duckdb ../data/sample.csv raw_staging
cargo run -p spatia_cli -- schema ./spatia.duckdb raw_staging
cargo run -p spatia_cli -- overture_extract ./spatia.duckdb places place -122.4,47.5,-122.2,47.7 places_wa
cargo run -p spatia_cli -- overture_search ./spatia.duckdb places_wa "lincoln" 20
```

### Quality gate (required before handoff)

```bash
pnpm build
cd src-tauri && cargo test --workspace && cargo clippy --workspace
```

### PMTiles build helper

```bash
bash src-tauri/scripts/build_overture_pmtiles.sh ./src-tauri/spatia.duckdb places_wa places ./out/places.pmtiles 6 14 1
```

## Architecture

### Stack

- **Frontend**: React 19 + TypeScript + Vite, Zustand, Radix UI
- **Map**: MapLibre GL + PMTiles vector tiles + Deck.gl overlays
- **Desktop shell**: Tauri v2 (command bridge between React and Rust)
- **Rust workspace** (`src-tauri/`):
  - `spatia_geocode` — geocoding logic (batch, cache, Geocodio, fuzzy search, scoring)
  - `spatia_ingest` — CSV data ingestion
  - `spatia_overture` — Overture Maps extract, search, and geocode
  - `spatia_ai` — Gemini client, prompt builders, data-cleaning orchestration (feature-gated via `gemini`)
  - `spatia_engine` — orchestration layer (analysis, schema, export, executor); re-exports geocode/ingest/overture APIs
  - `spatia_bench` — benchmarks for all modules (AI analysis + geocoding)
  - `spatia_cli` — CLI wrapper over `spatia_engine`'s executor
- **Database**: DuckDB 1.4.4 with `spatial` and `httpfs` extensions; fixed path `src-tauri/spatia.duckdb`

### Core runtime flows

**CSV Ingestion**: UI → Tauri `ingest_csv_with_progress` → engine → DuckDB table → progress events back to UI

**Analysis loop**: Chat submit → Tauri `analysis_chat` (schema-injected system prompt) → Gemini → `generate_analysis_sql` → `execute_analysis_sql` → creates `analysis_result` view → GeoJSON → MapLibre + Deck.gl overlay

**Geocoding**: Engine `geocode` is batch-first and local-first — fuzzy match against local Overture lookup table, then Geocodio HTTP fallback with persistent `geocode_cache` table. Returns confidence/source metadata per result.

**Map rendering**: MapLibre consumes PMTiles sources. `MapView` exposes a `MapViewHandle` ref (`getMap`) that `ChatCard` uses to execute imperative map actions (fly-to, fit-bounds, popups) via `executeMapActions` in `src/lib/mapActions.ts`.

**Overture extract**: `overture_extract` downloads bounded Overture parquet from S3 (via `httpfs`) into DuckDB tables used for search and geocoding.

### UI layout and state

The app shell (`src/App.tsx`) renders three components in a flat layout: `MapView` (full-bleed map), `FileList` (right panel — table management, CSV upload, geocoding), and `ChatCard` (floating chat bar — AI analysis). There is no router; the app is a single view.

State is managed in `src/lib/appStore.ts` (Zustand). The store holds `tables`, `chatMessages`, `analysisGeoJson`, and `mapActions`. `MapView` reads `analysisGeoJson` directly from the store to render result layers. `ChatCard` receives the `MapViewHandle` ref via props to call `executeMapActions` after each AI turn.

### Tauri command surface (`src-tauri/src/lib.rs`)

`ingest_csv_with_progress`, `analysis_chat`, `generate_analysis_sql`, `execute_analysis_sql`, `generate_visualization_command`, `geocode`, plus Overture and schema helpers.

### Engine executor command surface

`ingest`, `schema`, `overture_extract`, `overture_search`, `overture_geocode`, `geocode` — shared by CLI and Tauri.

## Key Constraints

- Do not rewrite core architecture or DB schemas without explicit permission.
- All SQL identifiers from user input must be validated via `identifiers.rs` before interpolation.
- Analysis SQL execution enforces a strict prefix: `CREATE [OR REPLACE] VIEW analysis_result AS ...`.
- DuckDB extensions (`spatial`, `httpfs`) are connection-scoped; load them per connection.
- Overture release must be pinned for reproducible extracts (`SPATIA_OVERTURE_RELEASE` env var or default in engine).
- Temp DuckDB files in tests must clean up `.duckdb`, `.wal`, and `.wal.lck`.
- `PRAGMA table_info` boolean fields map to `bool` (not `BOOLEAN` string).
- The app is a single-view layout (no router/sidebar). There are no separate page routes.
- DB file path is fixed at `src-tauri/spatia.duckdb`; no user-facing path input.

## Environment Variables

```
SPATIA_GEMINI_API_KEY        # Required for AI analysis paths
SPATIA_GEOCODIO_API_KEY      # Geocoding fallback
SPATIA_GEOCODIO_BATCH_SIZE   # Optional, default 100
SPATIA_GEOCODIO_BASE_URL     # Optional, for testing
SPATIA_OVERTURE_RELEASE      # Optional Overture release override
TAURI_DEV_HOST               # Vite HMR dev host (for non-localhost setups)
```

## Project Tracking Files

- `plan.md` — active task list; use it to track in-progress work
- `summary.md` — stable project notes, gotchas, and core paths (keep ≤400 words)
- `architecture.md` — detailed architecture reference
