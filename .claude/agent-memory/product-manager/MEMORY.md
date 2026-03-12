# Product Manager Agent Memory

## Project Identity
Spatia is a desktop GIS app (Tauri v2 + React 19 + Rust/DuckDB). Core value prop: upload CSV, geocode locally via Overture data, run AI spatial analysis via natural language, view results on map. Target user: data analysts and GIS practitioners who want an offline-capable, self-contained spatial analysis tool.

## Architectural Constraints (product-relevant)
- Two active routes: Map and Upload. Schema route exists but is deliberately hidden from navigation.
- Fixed DB: `src-tauri/spatia.duckdb`. No user-configurable DB path.
- AI SQL must produce `CREATE [OR REPLACE] VIEW analysis_result AS ...`. No mutation SQL allowed.
- Gemini only (no fallback provider). `SPATIA_GEMINI_API_KEY` is required for any AI feature.
- PMTiles are local files on disk — not streamed. Onboarding requires tile setup.
- Geocoding: local Overture fuzzy match first, Geocodio HTTP fallback second.

## Known Product Gaps (from plan.md, as of 2026-03-09)
- No user-facing error for missing `SPATIA_GEMINI_API_KEY` or missing PMTiles files
- Visualization commands: only `scatter` handled; `heatmap` and `hexbin` unimplemented
- Analysis SQL safety is prefix-only validation; deeper hardening is backlog
- No integration tests for Tauri analysis commands

## Key Files for Product Verification
- Active routes/pages: `src/pages/` and `src/components/`
- Main store: `src/lib/appStore.ts`
- Tauri command surface: `src-tauri/src/lib.rs`
- Engine commands: `src-tauri/crates/engine/src/`
- AI layer: `src-tauri/crates/ai/src/`
- Work tracking: `plan.md` (active), `summary.md` (stable notes)

## Recurring Risk Patterns
- Features touching AI analysis must account for: missing API key, empty result set, SQL that generates no geometry, Gemini rate limits
- Features touching geocoding must account for: no local Overture data loaded, Geocodio key absent, partial match confidence ambiguity
- Features touching the map must account for: PMTiles file not present, layer visibility state desync, Deck.gl overlay on top of MapLibre layer ordering

## Team Notes
- See `/Users/zhaotingzhou/Projects/spatia/.claude/agents/` for other agents: gis-tech-lead, senior-engineer, test-engineer, ui-design-architect, gis-domain-expert
- Detailed topics: see `architecture-notes.md` in this directory (create when needed)
