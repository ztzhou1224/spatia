# Spatia Architecture

## Strategic Direction

Spatia is a **local-first, AI-powered spatial intelligence platform** for data analysts who have spatial questions but no GIS background. The core value proposition: an analyst with a CSV of addresses and a spatial question gets a map answer in under 10 minutes, with no cloud dependencies, subscription fees, or GIS degree required.

**Market position:** Spatia fills the gap between ArcGIS Pro (too complex/expensive), Tableau (too limited on spatial), and Carto (enterprise-only, cloud-dependent). Carto's 2025 launch of AI Agents narrows what was previously a unique Spatia advantage on NL spatial queries, adding urgency to reach market with a complete product.

**Core differentiators:** (1) Local-first geocoding with persistent cache, (2) AI data cleaning (no competitor equivalent), (3) Zero-infrastructure setup -- no cloud DW, no enterprise license, (4) Offline operation as a compliance feature for sensitive data, (5) Integrated clean-geocode-analyze pipeline from raw CSV to map answer.

**Monetization model:** Free desktop core (acquisition channel) + Spatia Cloud ($15-25/user/month for managed tiles, AI, geocoding, exports, shareable maps) + Enterprise tier ($50-100/user/month for teams, SSO, audit logging, on-prem deployment). Local-first is the distribution strategy; cloud is the business model. Product gaps (setup friction, export, sharing) are the natural conversion funnel for cloud services.

**Target users:** Individual data analysts, market researchers, city planners, small teams at budget-constrained organizations -- anyone who has address data and spatial questions but cannot justify ArcGIS Pro licensing or Carto enterprise infrastructure.

## System Layers

1. **Frontend**: React 19 + TypeScript + Vite
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

UI upload -> Tauri `ingest_csv_with_progress` -> engine `ingest_csv(_to_table)` -> DuckDB table load -> auto-geocode if address columns detected -> auto-display on map -> progress events back to UI.

### Overture Local Data

Engine `overture_extract` builds bounded DuckDB tables from Overture parquet -> normalized lookup table for search/geocode relevance.

### Map Rendering

MapLibre consumes PMTiles vector sources and free raster basemaps (CartoDB Dark, CartoDB Positron, OpenStreetMap). Deck.gl overlays render scatter, heatmap, and hexbin layers from analysis results and ingested table data.

### Analysis Loop

Chat submit -> Tauri `analysis_chat` (schema-injected system prompt) -> Gemini response -> `generate_analysis_sql` -> SQL execution via `execute_analysis_sql` -> `analysis_result` view -> GeoJSON + tabular results -> rendered on map + Deck.gl overlay + inline result table.

### Visualization Command

`generate_visualization_command` returns structured JSON supporting scatter, heatmap, and hexbin layer types.

## UI Layout

Three-component flat layout in `src/App.tsx`:
- **MapView** (full-bleed map with Deck.gl overlays, legend panel, basemap selector)
- **FileList** (right panel -- table management, CSV upload, geocoding, chat context toggles)
- **ChatCard** (floating chat bar -- AI analysis, inline tabular results, new chat management)

State managed in `src/lib/appStore.ts` (Zustand): tables, chatMessages, analysisGeoJson, tableGeoJson, visualizationType, selectedTablesForChat, apiConfig.

## Stability / Safety Decisions

- Identifier validation enforced before SQL identifier interpolation.
- Analysis SQL execution restricted to `CREATE [OR REPLACE] VIEW analysis_result AS ...` prefix.
- Analysis SQL body scanned for 15 blocked keyword patterns (DROP, TRUNCATE, DELETE, ALTER, GRANT, etc.) using word-boundary regexes.
- Geocoding fallback is cache-first with persistent `geocode_cache` table.
- AI module is feature-gated (`gemini`) and supports explicit environment-based configuration.
- API key presence checked at startup with user-facing banners when missing.

## Shared Command Surface

Engine executor supports:

- `ingest <db_path> <csv_path> [table_name]`
- `schema <db_path> <table_name>`
- `overture_extract <db_path> <theme> <type> <bbox> [table_name]`
- `overture_search <db_path> <table_name> <query> [limit]`
- `overture_geocode <db_path> <table_name> <query> [limit]`
- `geocode <db_path> <address> [address2...]`

## Quality Gates

Before considering a task complete:

- `pnpm build`
- `cargo test --workspace`
- `cargo clippy --workspace`

## Critical Feature Gaps (from Market Fit Analysis)

### Pre-Launch Blockers (Phase 1 -- Table Stakes)

These gaps cause every target user to immediately identify Spatia as incomplete. All must be resolved before any public launch positioning.

1. **Data export** -- CSV export of any table, GeoJSON export of analysis_result, PNG export of map viewport
2. **Settings UI** -- In-app API key management (Tauri secure storage), PMTiles file picker, config verification
3. **Map legend** -- Auto-generated from active Deck.gl layer type, color encoding, data source name
4. **Map PNG export** -- MapLibre canvas capture via `map.getCanvas().toDataURL()`
5. **Basemap selector** -- Minimum: CartoDB Dark, CartoDB Positron, OpenStreetMap
6. **Truncation indicators** -- "Showing X of Y" badge on map and table when results are capped
7. **Tooltip labels** -- All UI controls labeled for discoverability

### Competitive Parity (Phase 2)

Features needed to compete favorably against Felt, Kepler.gl, and lower-tier ArcGIS/Tableau users.

1. GeoJSON/Shapefile import (DuckDB spatial `ST_Read`)
2. Column sort/filter in table preview
3. Editable SQL panel (show + edit AI-generated SQL before execution)
4. Chart export (PNG for bar/pie/histogram)
5. Example query suggestions in empty chat
6. Increased result limits with pagination (5K features, 100 table rows)
7. Line and time-series chart type
8. Map annotation (static text labels on features)

### Differentiation (Phase 3)

Features that capitalize on Spatia's unique architecture to create capabilities no competitor can easily replicate.

1. Spatial analysis wizard (buffer, intersect, point-in-polygon as guided UI flows)
2. Multi-layer map with user-controlled visibility and ordering
3. Choropleth / graduated symbol rendering
4. Saved analysis bookmarks (name + re-run a query)
5. Cross-filter (click map feature to filter chart, click chart bar to highlight map)
6. AI model configurability (OpenAI/Anthropic as alternatives to Gemini)
7. Overture data browser (explore nearby POIs, load additional Overture themes)
8. Temporal playback (animate points over a time column)

## Key Risks

1. **Setup friction** -- PMTiles installation and env var configuration will block non-technical target users. Mitigation: Settings UI (Phase 1) and eventual Spatia Cloud managed services.
2. **Single-provider AI dependency** -- Gemini-only coupling. Mitigation: abstract AI client layer for pluggable providers (Phase 3).
3. **AI SQL error dead end** -- No escape hatch when AI generates wrong SQL. Mitigation: editable SQL panel (Phase 2).
4. **1K feature GeoJSON limit** -- Silent truncation undermines analytical trust. Mitigation: truncation indicators (Phase 1), increased limits (Phase 2).
5. **Carto competitive pressure** -- AI Agents narrow Spatia's NL query advantage. Mitigation: speed to market on Phase 1, double down on offline/local-first/zero-infrastructure differentiators.
