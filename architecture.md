# Spatia Architecture

## Strategic Direction

Spatia is a **BYOK AI-native desktop app for insurance underwriters**. The core value proposition: analyze proprietary portfolio data against spatial risk layers, entirely on your machine, with AI that understands underwriting.

**Why insurance underwriting:** Property risk assessment requires location intelligence (flood, wildfire, crime overlays). Incumbent tools (SpatialKey/Insurity, RMS, AIR, Verisk) cost $100K+/year. Spatia's existing pipeline (CSV upload -> geocode -> spatial analysis -> AI-driven insights) maps directly to the underwriting workflow at a fraction of the price. Local-first architecture becomes a compliance feature for sensitive policy data.

**Competitive position:** Spatia fills the gap between ArcGIS Pro (too complex/expensive), Tableau (too limited on spatial), and Carto (enterprise-only, cloud-dependent). Carto's 2025 AI Agents launch narrows the NL spatial query advantage, adding urgency. Google Ask Maps (2026-03-12) validates "talk to a map" UX but targets consumers, not underwriters. Spatia's moat is local-first privacy + domain-specific AI + curated risk data subscription.

**Monetization model:** The app is the distribution vehicle. Curated hazard/risk datasets (wildfire, flood, wind, COPE) are the product, sold as a data subscription. A cracked app with stale data is worthless to a professional underwriter. BYOK model: users bring their own Gemini API key.

## System Layers

1. **Frontend**: React 19 + TypeScript + Vite
2. **Desktop Host**: Tauri v2 command bridge
3. **Backend Core**: Rust workspace (`spatia`, `spatia_engine`, `spatia_ai`, `spatia_cli`)
4. **Data Runtime**: DuckDB + spatial/httpfs + Overture data + local PMTiles artifacts
5. **Domain Layer**: Platform + DomainPack architecture (insurance_underwriting pack implemented)

## Workspace Structure

- `src/` - frontend UI
- `src-tauri/src/` - Tauri commands and app wiring
- `src-tauri/crates/engine/` - data/geo execution core + domain pack system
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

Chat submit -> Tauri `analysis_chat` (schema-injected system prompt + domain context) -> Gemini response -> `generate_analysis_sql` -> SQL execution via `execute_analysis_sql` -> `analysis_result` view -> GeoJSON + tabular results -> rendered on map + Deck.gl overlay + inline result table.

### Domain Pack Threading

All prompt builders accept `domain_context: Option<&str>`. When a domain pack is active (e.g., `SPATIA_DOMAIN_PACK=insurance_underwriting`), the domain pack's `system_prompt_extension` is injected into every AI call. Column detection rules identify domain-specific columns (financial, COPE, policy, risk) and annotate them in the prompt. Frontend reads `DomainPackConfig` at startup for UI customization.

### Visualization Command

`generate_visualization_command` returns structured JSON supporting scatter, heatmap, and hexbin layer types.

## Platform + Domain Pack Architecture (IMPLEMENTED)

The domain pack system separates platform capabilities from domain-specific behavior:

- **`DomainPack` struct** (`domain_pack.rs`): `system_prompt_extension`, `column_detection_rules`, `ui_config`
- **`DomainPack::generic()`**: Extracts current hardcoded values (zero behavioral change)
- **`DomainPack::insurance_underwriting()`**: Insurance-specific prompts, 24 column detection rules across 4 categories (financial, COPE, policy, risk), custom UI config
- **`DomainPack::from_env()`**: Resolves from `SPATIA_DOMAIN_PACK` env var, defaults to generic
- **Immutable for app lifetime** via OnceLock

### Adding a New Domain Pack

1. Add constructor to `DomainPack` in `domain_pack.rs`
2. Add match arm in `DomainPack::from_env()`
3. Define: system prompt extension, column detection rules, UI config

## UI Layout

Three-component flat layout in `src/App.tsx`:
- **MapView** (full-bleed map with Deck.gl overlays, legend panel, basemap selector)
- **FileList** (right panel -- table management, CSV upload, geocoding, chat context toggles)
- **ChatCard** (floating chat bar -- AI analysis, inline tabular results, new chat management)

State managed in `src/lib/appStore.ts` (Zustand): tables, chatMessages, analysisGeoJson, tableGeoJson, visualizationType, selectedTablesForChat, apiConfig, domainConfig.

## Stability / Safety Decisions

- Identifier validation enforced before SQL identifier interpolation.
- Analysis SQL execution restricted to `CREATE [OR REPLACE] VIEW analysis_result AS ...` prefix.
- Analysis SQL body scanned for 15 blocked keyword patterns (DROP, TRUNCATE, DELETE, ALTER, GRANT, etc.) using word-boundary regexes.
- Geocoding fallback is cache-first with persistent `geocode_cache` table.
- AI module is feature-gated (`gemini`) and supports explicit environment-based configuration.
- API key presence checked at startup with user-facing banners when missing.
- Domain pack is immutable for app lifetime (OnceLock) -- no runtime switching.

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
2. **Settings UI** -- In-app BYOK API key management (Tauri secure storage), PMTiles file picker, config verification
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

### Insurance Vertical + Differentiation (Phase 3)

Features that capitalize on Spatia's unique architecture and insurance positioning.

1. Risk layer data model and ingestion (FEMA flood zones, USGS wildfire, wind speed)
2. Hazard proximity analysis commands (distance-to-hazard, buffer zones, exposure aggregation)
3. Multi-layer map with user-controlled visibility and ordering
4. Data subscription manifest and loader (client-side infrastructure for risk data delivery)
5. AI model configurability (OpenAI/Anthropic as alternatives to Gemini)
6. Portfolio concentration analysis workflow
7. Single-risk assessment report (click point -> full risk profile)
8. Batch enrichment pipeline (enrich entire portfolio with all risk layer scores)
9. PDF risk assessment report generation

## Planned Rust Modules

- `src-tauri/crates/engine/src/risk_layers.rs` -- Risk layer ingestion and management
- `src-tauri/crates/engine/src/spatial_analysis.rs` -- Distance, buffer, aggregation functions
- `src-tauri/crates/engine/src/data_subscription.rs` -- Manifest + download client
- `src-tauri/crates/engine/src/export.rs` -- CSV and GeoJSON export
- `src-tauri/src/license.rs` -- License check + offline grace period

## Planned Frontend Components

- `src/components/SettingsPanel.tsx` -- BYOK key management
- `src/components/MapLegend.tsx` -- Auto-generated map legend
- `src/components/DataCatalog.tsx` -- Subscription data layer browser
- `src/components/LayerPanel.tsx` -- Multi-layer visibility controls

## Key Risks

1. **Setup friction** -- PMTiles installation and env var configuration will block non-technical users. Mitigation: Settings UI (Phase 1) and eventual managed data subscription.
2. **Single-provider AI dependency** -- Gemini-only coupling. Mitigation: abstract AI client layer for pluggable providers (Phase 3).
3. **AI SQL error dead end** -- No escape hatch when AI generates wrong SQL. Mitigation: editable SQL panel (Phase 2).
4. **1K feature GeoJSON limit** -- Silent truncation undermines analytical trust. Mitigation: truncation indicators (Phase 1), increased limits (Phase 2).
5. **Carto competitive pressure** -- AI Agents narrow Spatia's NL query advantage. Mitigation: speed to market on Phase 1, lean into insurance vertical differentiation that Carto does not serve.
6. **Risk layer data model** -- Not yet built. Foundational for Phases 2-3 and the data subscription monetization model.
