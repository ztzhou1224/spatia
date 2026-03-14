---
name: project_spatia
description: Key Spatia-specific domain knowledge for GIS expert review work
type: project
---

## Pipeline data flow

CSV upload -> DuckDB table -> optional AI clean -> geocoding (local Overture fuzzy match -> Geocodio fallback) -> analysis SQL via `build_unified_chat_prompt` -> `CREATE OR REPLACE VIEW analysis_result AS ...` -> GeoJSON -> MapLibre + Deck.gl overlay.

**Why:** Understanding the full pipeline is essential to evaluate where real-world data breaks and what test cases matter.
**How to apply:** When evaluating any feature or generating test cases, trace the data through each stage and identify failure modes at each handoff point.

## AI prompt architecture (as of MVP)

- `build_unified_chat_prompt` is the main analysis prompt — multi-table, conversation-aware, returns structured JSON with `message`, `sql`, `visualization_type`, `map_actions`
- Geocoded tables use `_lat` and `_lon` column naming convention
- Visualization types: scatter, heatmap, hexbin (all frontend-rendered from raw lat/lon rows), table, bar_chart, pie_chart, histogram
- H3 and ST_HexagonGrid are explicitly blocked in all prompts
- Multi-step intermediate views (`_spatia_step_1` etc.) are supported up to 5 steps

## Key spatial limitations in DuckDB

- No H3 functions — frontend handles hex binning
- No ST_HexagonGrid or ST_SquareGrid
- ST_Distance returns degrees when inputs are WGS84 lat/lon — not meters. This is a major pitfall for distance-based queries.
- Spatial extension does support: ST_Point, ST_Buffer, ST_Within, ST_Intersects, ST_Distance, ST_DWithin (check availability)

## Overture data realities for Spatia

- `places` theme = POIs/businesses, NOT a street address dataset — local geocoding coverage limited
- `buildings` theme = footprints, useful for spatial joins
- `transportation` theme = road segments, needs topology for network analysis
- Bounding box extracts can be 500K+ rows for metro areas
- Geocoded column naming: `_lat` / `_lon` appended to original table

## Test scenario work (2026-03-12)

First comprehensive test scenario document created covering 6 personas, spatial reasoning edge cases, AI pitfall taxonomy, and quality indicators. See response in that session for full detail.
