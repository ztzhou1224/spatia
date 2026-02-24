# Spatia Architecture (Current)

## System Layers

1. **Frontend**: React + TypeScript + Vite
2. **Desktop Host**: Tauri v2 command bridge
3. **Backend Core**: Rust workspace (`spatia`, `spatia_engine`, `spatia_ai`, `spatia_cli`)
4. **Data Runtime**: DuckDB + spatial/httpfs + Overture data + local PMTiles artifacts

## Workspace Structure

- `src/` - frontend UI
- `src-tauri/src/` - Tauri commands and app wiring
- `src-tauri/crates/engine/` - data/geo execution core
- `src-tauri/crates/ai/` - Gemini client + prompt builders + cleaner logic
- `src-tauri/crates/cli/` - CLI wrapper over engine command surface

## Core Runtime Flows

### Ingestion

UI upload -> Tauri `ingest_csv_with_progress` -> engine `ingest_csv(_to_table)` -> DuckDB table load -> progress events back to UI.

### Overture Local Data

Engine `overture_extract` builds bounded DuckDB tables from Overture parquet -> normalized lookup table for search/geocode relevance.

### Map Rendering

MapLibre consumes PMTiles vector sources; layer visibility is controlled in UI and reflected into widget metadata.

### Analysis Loop

Chat submit -> Tauri `analysis_chat` (schema-injected system prompt) -> Gemini response.

Goal-driven SQL generation -> `generate_analysis_sql` -> SQL execution via `execute_analysis_sql` -> `analysis_result` view -> GeoJSON -> rendered on map + Deck.gl overlay.

### Visualization Command

`generate_visualization_command` returns structured JSON (currently mapped to scatter baseline, extensible for heatmap/hexbin).

## Stability / Safety Decisions

- Identifier validation enforced before SQL identifier interpolation.
- Analysis SQL execution restricted to `CREATE [OR REPLACE] VIEW analysis_result AS ...` prefix.
- Geocoding fallback is cache-first with persistent `geocode_cache` table.
- AI module is feature-gated (`gemini`) and supports explicit environment-based configuration.

## Shared Command Surface

Engine executor supports:

- `ingest <db_path> <csv_path> [table_name]`
- `schema <db_path> <table_name>`
- `overture_extract <db_path> <theme> <type> <bbox> [table_name]`
- `overture_search <db_path> <table_name> <query> [limit]`
- `overture_geocode <db_path> <table_name> <query> [limit]`
- `geocode <db_path> <address> [address2...]`

## Focus System

- Zustand widget store tracks widget registry, app focus, and metadata.
- `useFocusGuard` captures pointer-down app focus for map/search/chat widgets.
- Map runtime syncs camera/layer/selection metadata into store.
- Chat context is derived from `lastNonChatFocusedWidgetId` via `buildAIContext`.

## Quality Gates

Before considering a task complete:

- `pnpm build`
- `cargo test --workspace`
- `cargo clippy --workspace`
