# GIS Tech Lead - Agent Memory

## Team Composition
- 6 agents: gis-tech-lead (opus), senior-engineer (sonnet), product-manager (sonnet), gis-domain-expert (sonnet), test-engineer, ui-design-architect
- Tech lead is the only opus-model agent -- use for architecture/planning, delegate implementation to sonnet agents
- senior-engineer handles all hands-on coding across the full stack
- product-manager should be consulted first for ambiguous or broad feature requests

## Codebase Structure (verified 2026-03-09)
- Frontend: flat `src/components/`: ChatCard.tsx, FileList.tsx, MapView.tsx
- State: `src/lib/appStore.ts` (Zustand), `src/lib/mapActions.ts`, `src/lib/constants.ts`, `src/lib/tauri.ts`
- Old widget system (widgetStore.ts, useFocusGuard.ts, aiContext.ts, src/pages/*) is DELETED
- Engine modules: executor.rs, analysis.rs, geocode.rs, overture.rs, schema.rs, ingest.rs, identifiers.rs, types.rs, db_manager.rs
- AI modules: client.rs, prompts.rs, cleaner.rs (all behind `gemini` feature gate)
- Tauri commands in `src-tauri/src/lib.rs` -- 15 registered handlers
- DB path resolved at startup via OnceLock, defaults to app-data dir
- geocode_integration_tests.rs exists and is wired in (59 tests passing)

## Quality Gate Status (2026-03-09)
- All 3 gates passing: pnpm build, cargo test (59 tests), cargo clippy (clean)
- Known non-blocking bundler warning: @loaders.gl "spawn" import

## Key Architectural Patterns
- Tauri commands defined directly in lib.rs (not split into modules)
- Engine uses string-command executor shared between CLI and Tauri
- AI crate feature-gated behind `gemini` flag (default=on)
- Analysis SQL: `CREATE [OR REPLACE] VIEW analysis_result AS ...` prefix enforced
- Unified chat_turn: multi-table schemas + conversation history -> Gemini JSON -> SQL exec -> GeoJSON + map_actions
- Geocoding: cache -> Overture local fuzzy -> Geocodio HTTP fallback
- All user-input SQL identifiers validated through identifiers.rs

## Testing Infrastructure (researched 2026-03-09)
- Tauri v2 official WebDriver does NOT work on macOS (only Linux/Windows)
- Community crate `tauri-plugin-webdriver-automation` provides W3C WebDriver for macOS WKWebView
  - GitHub: danielraffel/tauri-webdriver, published Feb 2026
  - Companion MCP server: mcp-tauri-automation (for AI agent integration)
- macOS native screenshot: `screencapture -l<WID> -x <file>` works for capturing specific windows
  - Window IDs obtainable via Swift CGWindowListCopyWindowInfo (verified working)
  - Syntax: `-l410` (no space between flag and ID)
- For UI state: Tauri `webview.eval()` can run JS to serialize Zustand store
- Python 3.9.10 available; no PyObjC (Quartz module missing)
- Node v25.6.1 available for WebDriverIO tests

## MVP Phase 1 Status (ALL COMPLETE as of 2026-03-11)
- All 17 tasks (P0 + Phases 1-4) DONE except TASK-P0-3 (WebDriver E2E, deferred)
- Quality gates passing, bundler warning resolved, 59+ tests

## Phase 2 Plan (2026-03-11)
Five areas planned, ~180-240h total:
1. **UI Overhaul** (L, 40-50h): shadcn/ui adoption, resizable panels (react-resizable-panels), floating widget system, glassmorphic polish
2. **AI Viz Widgets** (L, 35-45h): Recharts for bar/pie/histogram, standalone data table widgets, named map overlays, AI selects widget type
3. **Multi-step Query + Retry** (M-L, 30-40h): multi-statement SQL (temp tables with _temp_ prefix), auto error retry (max 2), single-connection execution
4. **Ingest UX** (M, 20-25h): granular progress events, geocode confirmation dialog, error display polish
5. **Project/Workspace** (XL, 45-55h): one DuckDB per project, projects.json index, RwLock replaces OnceLock for DB_PATH, migration from single-DB

Implementation order: UI -> Multi-step -> Ingest -> Viz Widgets -> Project

Key architectural decisions:
- DB_PATH must change from OnceLock to RwLock for multi-project
- Multi-step SQL: intermediates must be CREATE TEMP TABLE/VIEW _temp_*, final must be analysis_result view
- Charting: Recharts (React-native, ~45KB, code-split)
- Widget system: new widgetStore.ts (simpler than old deleted one -- just position/size/visibility)
- Panel layout: react-resizable-panels + MapLibre resize() on panel change

## Key Docs
- `.claude/agent-testing-guide.md` -- how each agent uses testing/verification tools
- Agent definitions in `.claude/agents/` -- test-engineer, product-manager, ui-design-architect all reference the guide

See also: [codebase-patterns.md](codebase-patterns.md) for detailed technical patterns.
