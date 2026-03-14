# Spatia Summary (Stable)

## Purpose

Quick-start memory file for constraints, invariants, and daily commands.

## Strategic Direction

Local-first, AI-powered spatial intelligence for data analysts without GIS backgrounds. Target: analysts with address data and spatial questions who cannot justify ArcGIS Pro or Carto enterprise pricing. Monetization: free desktop (acquisition) + Spatia Cloud ($15-25/user/mo) + Enterprise ($50-100/user/mo). Local-first = distribution strategy + compliance feature; cloud = business model.

## Current Stack

- Frontend: React 19 + TypeScript + Vite, Radix UI, Zustand
- Desktop shell: Tauri v2
- Rust crates: `spatia_engine` (core), `spatia_ai` (Gemini, feature-gated), `spatia_cli`
- Database: DuckDB + `spatial` / `httpfs`
- Map runtime: MapLibre + PMTiles + Deck.gl (scatter, heatmap, hexbin)

## Non-Negotiables

- Do not rewrite core architecture or DB schemas without explicit permission.
- Keep Rust code warning-free and memory-safe.
- Validate all SQL identifiers from user input via `identifiers.rs`.
- Quality gate before handoff: `pnpm build` then `cargo test --workspace && cargo clippy --workspace`.

## High-Value Gotchas

1. `PRAGMA table_info` boolean fields map to `bool`.
2. DuckDB extensions are connection-scoped; load per connection.
3. Overture release pinning required for reproducible extracts.
4. Temp DuckDB test cleanup: `.duckdb`, `.wal`, `.wal.lck`.
5. Analysis SQL: prefix enforced + 15-pattern blocked keyword scan (word-boundary regexes).
6. DB path fixed at `src-tauri/spatia.duckdb`; no user-facing path input.
7. Geocoding: batch-first, local-first (Overture fuzzy -> Geocodio fallback), returns confidence/source metadata.

## Core Paths

- App shell: `src/App.tsx` -- three-component flat layout (MapView, FileList, ChatCard)
- Components: `src/components/MapView.tsx`, `FileList.tsx`, `ChatCard.tsx`
- State: `src/lib/appStore.ts` (tables, chatMessages, analysisGeoJson, tableGeoJson, visualizationType, selectedTablesForChat, apiConfig)
- Map actions: `src/lib/mapActions.ts`
- Tauri commands: `src-tauri/src/lib.rs`
- Engine: `src-tauri/crates/engine/src/`
- AI: `src-tauri/crates/ai/src/`

## Operational Commands

- Dev: `pnpm tauri dev`
- Build: `pnpm build`
- Test: `cargo test --workspace`
- Lint: `cargo clippy --workspace`

## Active Risks

- Pre-launch blockers: no data export, no settings UI, no map legend, no map export (see `plan.md` Phase 1).
- Single-provider AI dependency (Gemini only).
- Carto AI Agents narrowing NL spatial query differentiation -- speed to market critical.
- Setup friction (PMTiles + env vars) blocks non-technical users.
- 1K feature limit causes silent truncation on real-world datasets.
