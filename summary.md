# Spatia Summary (Stable)

## Purpose

Quick-start memory file for constraints, invariants, and daily commands.

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
5. Analysis SQL execution validates prefix (`CREATE [OR REPLACE] VIEW analysis_result AS ...`) and scans the body for 15 blocked keyword patterns (DROP, TRUNCATE, DELETE, ALTER, GRANT, etc.) using word-boundary regexes to avoid false positives on column names.
6. User-facing DB path inputs are removed; app flows use fixed DB file path `src-tauri/spatia.duckdb`.
7. Engine `geocode` command is now batch-first and local-first: it uses local fuzzy matching from Overture lookup tables before Geocodio fallback, and returns confidence/source metadata per result.

## Core Paths

- App shell: `src/App.tsx` — three-component flat layout (MapView, FileList, ChatCard)
- Components: `src/components/MapView.tsx`, `src/components/FileList.tsx`, `src/components/ChatCard.tsx`
- State store: `src/lib/appStore.ts` (Zustand — tables, chatMessages, analysisGeoJson, tableGeoJson, visualizationType, selectedTablesForChat, apiConfig)
- Map actions: `src/lib/mapActions.ts` (executeMapActions over MapLibre ref)
- Tauri commands: `src-tauri/src/lib.rs`
- Engine core: `src-tauri/crates/engine/src`
- AI prompts/client: `src-tauri/crates/ai/src`

## Operational Commands

- Tauri dev: `pnpm tauri dev`
- Frontend build: `pnpm build`
- Rust tests: `cargo test --workspace`
- Rust lint: `cargo clippy --workspace`

## Active Risks

- AI env setup (`SPATIA_GEMINI_API_KEY`) and local PMTiles presence need clearer UX diagnostics (partially addressed with API key banners in FileList/ChatCard).
