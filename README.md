# Spatia

Spatia is a desktop GIS app built with Tauri + React and a Rust/DuckDB backend.

It supports CSV ingestion, Overture-backed local search/geocode workflows, PMTiles map rendering, schema-aware AI analysis, and widget focus/context tracking for map-centric chat interactions.

## Repository Layout

- `src/` - frontend app (routes, map UI, chat UI)
- `src-tauri/` - Tauri host app
- `src-tauri/crates/engine/` - data engine (`spatia_engine`)
- `src-tauri/crates/ai/` - AI helpers/prompts (`spatia_ai`)
- `src-tauri/crates/cli/` - CLI wrapper (`spatia_cli`)

## Prerequisites

- Node + pnpm
- Rust toolchain
- Tauri prerequisites for your OS

Optional for full map-data workflow:

- `duckdb` CLI
- `tippecanoe`

## Development

### Run desktop app

```bash
pnpm tauri dev
```

### Build frontend

```bash
pnpm build
```

### Rust validation

```bash
cd src-tauri
cargo test --workspace
cargo clippy --workspace
```

## CLI Quick Commands

```bash
cd src-tauri
cargo run -p spatia_cli -- ingest ./spatia.duckdb ../data/sample.csv raw_staging
cargo run -p spatia_cli -- schema ./spatia.duckdb raw_staging
cargo run -p spatia_cli -- overture_extract ./spatia.duckdb places place -122.4,47.5,-122.2,47.7 places_wa
cargo run -p spatia_cli -- overture_search ./spatia.duckdb places_wa "lincoln" 20
```

## Environment Variables

### AI

- `SPATIA_GEMINI_API_KEY` - enables Gemini-backed analysis/cleaning paths.

### Geocoding fallback

- `SPATIA_GEOCODIO_API_KEY`
- `SPATIA_GEOCODIO_BATCH_SIZE` (optional)
- `SPATIA_GEOCODIO_BASE_URL` (optional, testing)

### Overture release override

- `SPATIA_OVERTURE_RELEASE` (optional)

## PMTiles Build Helper

```bash
bash src-tauri/scripts/build_overture_pmtiles.sh ./src-tauri/spatia.duckdb places_wa places ./out/places.pmtiles 6 14 1
```

## Current Notes

- Analysis SQL execution targets `analysis_result` view generation and GeoJSON rendering.
- Visualization command parsing is wired with a scatter baseline in Deck.gl.
- Widget focus/context system is active for map/search/chat context handoff.
