# Spatia Summary (Stable)

## Purpose

Quick-start memory file for constraints, invariants, and daily commands.

## Product Direction (as of 2026-03-14)

**Pivot**: Spatia is now a **BYOK AI-native desktop app for insurance underwriters**. The core value proposition: analyze proprietary portfolio data against spatial risk layers, entirely on your machine, with AI that understands underwriting.

**Monetization**: The app is the distribution vehicle. Curated hazard/risk datasets (wildfire, flood, wind, COPE) are the product, sold as a data subscription. A cracked app with stale data is useless to a professional underwriter.

**Competitive moat**: Local-first privacy + proprietary data analysis + domain-specific AI (underwriter expert agent) + curated risk data subscription. Google Ask Maps (launched 2026-03-12) validates "talk to a map" UX but targets consumers, not underwriters.

## Feature Development Process (MANDATORY)

Every feature must pass: PROPOSE (PM) -> VALIDATE (Underwriter Expert) -> EVIDENCE (web search for real-world scenarios) -> REFINE -> SPEC (Tech Lead) -> BUILD (Engineer) -> VERIFY (Test + PM + Underwriter Expert). No feature ships without underwriter domain validation and real-world evidence.

## Current Stack

- Frontend: React 19 + TypeScript + Vite, Radix UI, Zustand
- Desktop shell: Tauri v2
- Rust crates:
  - `spatia_engine` (core data + geospatial logic)
  - `spatia_ai` (Gemini client + prompts + cleaner helpers)
  - `spatia_cli` (CLI wrapper)
- Database: DuckDB + `spatial` (and `httpfs` when needed)
- Map runtime: MapLibre + PMTiles + Deck.gl overlay

## Non-Negotiables

- Do not rewrite core architecture or DB schemas without explicit permission.
- Keep Rust code warning-free and memory-safe.
- Validate all SQL identifiers from user input.
- Preserve test/lint gate before handoff:
  1. `pnpm build`
  2. `cargo test --workspace`
  3. `cargo clippy --workspace`

## High-Value Gotchas

1. `PRAGMA table_info` boolean fields map to `bool`.
2. DuckDB extensions are connection-scoped; load per connection.
3. Overture release pinning is required for reproducible extracts.
4. Temp DuckDB test cleanup should remove `.duckdb`, `.wal`, `.wal.lck`.
5. Analysis SQL execution validates prefix (`CREATE [OR REPLACE] VIEW analysis_result AS ...`) and scans the body for 15 blocked keyword patterns using word-boundary regexes.
6. User-facing DB path inputs are removed; app uses fixed DB file path `src-tauri/spatia.duckdb`.
7. Engine `geocode` is batch-first and local-first: Overture fuzzy match -> Geocodio fallback -> persistent cache.

## Core Paths

- App shell: `src/App.tsx` — three-component flat layout (MapView, FileList, ChatCard)
- Components: `src/components/MapView.tsx`, `src/components/FileList.tsx`, `src/components/ChatCard.tsx`
- State store: `src/lib/appStore.ts` (Zustand — tables, chatMessages, analysisGeoJson, tableGeoJson, visualizationType, selectedTablesForChat, apiConfig)
- Map actions: `src/lib/mapActions.ts` (executeMapActions over MapLibre ref)
- Tauri commands: `src-tauri/src/lib.rs`
- Engine core: `src-tauri/crates/engine/src`
- AI prompts/client: `src-tauri/crates/ai/src`

## Agent Team

| Agent | Role |
|-------|------|
| senior-engineer | Full-stack implementation |
| gis-tech-lead | Architecture, specs, coordination |
| underwriter-expert (NEW) | Domain validation gate, industry expertise |
| product-manager | User stories, acceptance, verification |
| test-engineer | TDD, integration tests, E2E |
| ui-design-architect | Component design, UX |
| gis-domain-expert | Spatial analysis advisory |

## Active Risks

- AI env setup (`SPATIA_GEMINI_API_KEY`) needs clearer UX diagnostics (partially addressed with banners).
- Risk layer data model and subscription infrastructure not yet built.
- Underwriter system prompt not yet implemented — chat is still generic GIS mode.
