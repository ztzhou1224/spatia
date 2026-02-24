# Spatia Development Plan (Session-Ready)

## Status Snapshot

- Phases 1-6 are implemented and validated.
- Latest quality gate: `cargo test --workspace` + `cargo clippy --workspace` passing.
- Frontend build is passing (`pnpm build`).

## Completed Milestones

- Rust core engine + CLI split (`spatia_engine`, `spatia_cli`) with DuckDB + spatial.
- String-command executor for shared CLI/Tauri command surface.
- Overture extract/search/geocode flow and PMTiles-oriented map integration.
- Geocodio fallback geocoding with persistent DuckDB cache.
- AI cleaner pipeline (Gemini-gated) with schema/sample context and SQL safety checks.
- Frontend upload/map experiences (file picker, ingestion progress, PMTiles layers, search).
- Analysis loop: schema-aware chat, AI SQL generation (`analysis_result`), SQL execution to GeoJSON, map rendering, visualization command parsing.
- Widget focus system foundation (`useWidgetStore`, `useFocusGuard`, map metadata sync, chat context pill/context serialization, app-focus ring).

## Next Session Backlog (Prioritized)

- [ ] Remove/resolve frontend bundler warning from Deck/loaders worker import (`spawn` via Vite browser external) and verify production-safe behavior.
- [ ] Add integration tests for Tauri analysis commands:
  - `analysis_chat`
  - `generate_analysis_sql`
  - `execute_analysis_sql`
  - `generate_visualization_command`
- [ ] Harden SQL execution safety for analysis flow (allowlist + stricter parser checks beyond view-prefix validation).
- [ ] Expand visualization command handling beyond `scatter`:
  - parse + map `heatmap`
  - parse + map `hexbin`
  - fallback behavior when command is unsupported
- [ ] Add minimal user-facing diagnostics for missing AI config (`SPATIA_GEMINI_API_KEY`) and missing local PMTiles files.
- [ ] Add concise architecture diagram/flow section to README for new contributors.

## Notes for Next Agent

- Keep architecture intact; avoid schema rewrites unless explicitly requested.
- Preserve strict validation gates before closing work:
  1. `pnpm build`
  2. `cargo test --workspace`
  3. `cargo clippy --workspace`
