# GIS Tech Lead - Agent Memory

## Team Composition
- 7 agents: gis-tech-lead (opus), senior-engineer (sonnet), product-manager (sonnet), gis-domain-expert (sonnet), test-engineer, ui-design-architect, underwriter-expert (NEW)
- Tech lead is the only opus-model agent -- use for architecture/planning, delegate implementation to sonnet agents
- senior-engineer handles all hands-on coding across the full stack
- product-manager should be consulted first for ambiguous or broad feature requests
- underwriter-expert is the domain validation gate -- all insurance features must pass through

## Strategic Direction (updated 2026-03-14)
- **Pivot**: BYOK AI-native desktop app for insurance underwriters
- **Monetization**: App is distribution vehicle; curated risk data subscription is the product
- **Competitive context**: Carto AI Agents (2025) narrow NL spatial query advantage; Google Ask Maps (2026-03-12) validates "talk to a map" but targets consumers
- **Market fit analysis** (`market-fit-analysis.md`): identifies 7 critical pre-launch blockers (no export, no settings UI, no legend, no map export, no basemap selector, no truncation indicators, no tooltip labels)
- **Feature dev process**: PROPOSE -> VALIDATE (underwriter) -> EVIDENCE -> REFINE -> SPEC -> BUILD -> VERIFY

## Codebase Structure (verified 2026-03-14)
- Frontend: flat `src/components/`: ChatCard.tsx, FileList.tsx, MapView.tsx
- State: `src/lib/appStore.ts` (Zustand) -- includes domainConfig from DomainPack
- Engine modules: executor.rs, analysis.rs, geocode.rs, overture.rs, schema.rs, ingest.rs, identifiers.rs, types.rs, db_manager.rs, **domain_pack.rs** (NEW)
- AI modules: client.rs, prompts.rs, cleaner.rs (all behind `gemini` feature gate)
  - All prompt builders now accept `domain_context: Option<&str>` (zero-cost when None)
- Tauri commands in `src-tauri/src/lib.rs` -- includes `get_domain_pack_config`
- DomainPack: OnceLock, resolved from `SPATIA_DOMAIN_PACK` env var at startup
  - `DomainPack::generic()` -- default, extracts current hardcoded values
  - `DomainPack::insurance_underwriting()` -- 24 column detection rules, insurance system prompt

## Key Architectural Patterns
- Tauri commands defined directly in lib.rs (not split into modules)
- Engine uses string-command executor shared between CLI and Tauri
- AI crate feature-gated behind `gemini` flag (default=on)
- Analysis SQL: `CREATE [OR REPLACE] VIEW analysis_result AS ...` prefix enforced + 15-pattern blocklist
- Unified chat_turn: multi-table schemas + domain context + conversation history -> Gemini JSON -> SQL exec -> GeoJSON + map_actions
- Geocoding: cache -> Overture local fuzzy -> Geocodio HTTP fallback
- Domain pack is immutable for app lifetime (OnceLock) -- no runtime switching
- All user-input SQL identifiers validated through identifiers.rs

## Current Plan State (2026-03-14)
- MVP Sprint: ALL COMPLETE (13 tasks + 3 P0 tasks)
- plan.md now has: Platform+DomainPack phase (COMPLETE) + Table Stakes (TASK-14-21, NOT STARTED) + Insurance Underwriting (TASK-UW-03/04, NOT STARTED) + BYOK/Subscription (TASK-SUB-01/02/03, NOT STARTED) + Workflows (TASK-WF-01/02/03, NOT STARTED) + Competitive Parity (TASK-22-26, NOT STARTED)
- Priority sequencing: Table stakes first, then risk layer infra, then competitive parity, then workflows
- Market fit analysis integrated into plan.md strategic context

## Key Docs
- `market-fit-analysis.md` -- comprehensive competitive analysis vs ArcGIS Pro, Tableau, Carto
- `architecture.md` -- now reflects insurance pivot + DomainPack architecture
- `summary.md` -- under 400 words, reflects pivot + active risks
- `.claude/agent-testing-guide.md` -- how each agent uses testing/verification tools

See also: [codebase-patterns.md](codebase-patterns.md) for detailed technical patterns.
