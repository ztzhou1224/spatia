# Spatia Summary (Stable)

## Purpose

Quick-start memory file for constraints, invariants, and daily commands.

## Current Stack

- Frontend: React + TypeScript + Vite
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
5. Analysis SQL execution currently validates prefix only (`CREATE [OR REPLACE] VIEW analysis_result AS ...`); deeper hardening remains a backlog item.

## Core Paths

- Frontend pages/components: `src/pages`, `src/components`
- Focus/context system: `src/lib/widgetStore.ts`, `src/lib/useFocusGuard.ts`, `src/lib/aiContext.ts`
- Tauri commands: `src-tauri/src/lib.rs`
- Engine core: `src-tauri/crates/engine/src`
- AI prompts/client: `src-tauri/crates/ai/src`

## Operational Commands

- Tauri dev: `pnpm tauri dev`
- Frontend build: `pnpm build`
- Rust tests: `cargo test --workspace`
- Rust lint: `cargo clippy --workspace`

## Active Risks

- Deck/loaders bundler warning in production build should be resolved/verified.
- Visualization command support is only scatter-level in the Deck.gl mapping path.
- AI env setup (`SPATIA_GEMINI_API_KEY`) and local PMTiles presence need clearer UX diagnostics.
