# Spatia Pivot Plan — BYOK AI-Native Desktop App for Insurance Underwriters

## Date: 2026-03-14

## Executive Summary

Spatia pivots from a general-purpose GIS desktop app to a **BYOK (Bring Your Own Key) AI-native desktop application** purpose-built for **insurance underwriters**. The app's core value proposition is: **analyze your proprietary portfolio data against spatial risk layers, entirely on your machine, with AI that understands underwriting.**

### Strategic Context (from founder discussions)

1. **Monetization model**: The app itself is the distribution vehicle. The **curated hazard/risk datasets are the product**. Users need an active data subscription to get fresh wildfire, flood, wind, COPE, and other risk layers. A cracked app with stale data is worthless to a professional underwriter.

2. **Google Ask Maps is not a threat**: Google's Ask Maps (launched 2026-03-12) is a consumer discovery tool ("find me a restaurant"). Spatia solves a fundamentally different job: "analyze my proprietary book of business against hazard data." Google can't touch proprietary data analysis, and their cloud-first model conflicts with our local-first privacy guarantee. Google actually _helps_ us by training the market to expect "talk to a map" interactions.

3. **Competitive moat**: Local-first privacy (data never leaves the machine) + proprietary data analysis + domain-specific AI (underwriting expert agent) + curated risk data subscription. None of these overlap with consumer map products.

---

## What We Have (MVP Sprint 1 — COMPLETE)

All quality gates passing. 13/14 tasks completed.

### Working End-to-End
- CSV ingestion with auto table naming
- AI-powered data cleaning via Gemini
- Address detection + auto-geocoding (Overture local → Geocodio fallback)
- Auto-display geocoded data on map after pipeline
- Unified chat_turn: schema injection → Gemini → SQL → GeoJSON → map layers
- Map rendering: MapLibre + PMTiles + Deck.gl (scatter, heatmap, hexbin)
- Map actions from AI: fly_to, fit_bounds, show_popup, highlight_features
- Tabular results display in chat
- Table selection for chat context
- Chat history management (clear, 50-message cap)
- API key degradation banners
- Empty state / onboarding UX
- SQL safety (identifier validation + 15-pattern blocklist)
- Code-split bundle (maplibre + deckgl chunks)

---

## Feature Development Process (MANDATORY)

Every feature request or user story MUST go through this pipeline before implementation begins:

```
1. PROPOSE  →  Product Manager drafts user story + acceptance criteria
2. VALIDATE →  Underwriter Domain Expert reviews for industry relevance
                - Does this solve a real underwriting workflow problem?
                - Is the terminology correct?
                - Are the assumptions about data/process accurate?
3. EVIDENCE →  Web search for real-world validation
                - Find evidence this workflow exists in practice
                - Check competitor products (RMS, AIR, Verisk, Cape Analytics, Nearmap)
                - Look for industry standards (ACORD, ISO, COPE frameworks)
                - Document 2-3 real-world scenarios where this feature applies
4. REFINE   →  Adjust story based on expert + evidence feedback
5. SPEC     →  Tech Lead creates technical spec with tasks
6. BUILD    →  Senior Engineer implements
7. VERIFY   →  Test Engineer + Product Manager verify against acceptance criteria
                Underwriter Expert confirms the result matches real-world expectations
```

### Why this matters
Building features that _feel_ right to engineers but don't match how underwriters actually work is the #1 risk for this product. The domain expert gate + evidence search prevents us from building fantasy features. If we can't find real-world evidence that a workflow exists, we don't build it.

### Agent responsibilities in this pipeline
- **product-manager**: Steps 1, 4, 7 — owns the story from draft to acceptance
- **underwriter-expert (NEW)**: Steps 2, 7 — domain validation gate, must approve before build
- **gis-tech-lead**: Step 5 — technical spec and task breakdown
- **senior-engineer**: Step 6 — implementation
- **test-engineer**: Step 7 — automated verification
- **Any agent**: Step 3 — web search for evidence (use WebSearch tool to find industry references)

---

## Phase 1: Platform + Domain Pack Architecture (Priority: CRITICAL — COMPLETE)

Goal: Refactor Spatia into a clean Platform + Domain Pack architecture, then implement the insurance underwriting domain pack.

### TASK-PLAT-01: DomainPack Abstraction (COMPLETE)
- **Implementation**: Created `DomainPack` struct in `spatia_engine::domain_pack` with:
  - `system_prompt_extension` — domain expertise injected into AI chat
  - `column_detection_rules` — patterns to recognize industry columns (category, patterns, display_label)
  - `ui_config` — assistant name, placeholder text, colors, map defaults
  - `DomainPack::generic()` — extracts current hardcoded values (zero behavioral change)
  - `DomainPack::insurance_underwriting()` — first domain pack
  - `DomainPack::from_env()` — resolves from `SPATIA_DOMAIN_PACK` env var
- **Files**: `src-tauri/crates/engine/src/domain_pack.rs` (new), `src-tauri/crates/engine/src/lib.rs`

### TASK-PLAT-02: Domain Context Threading (COMPLETE)
- **Implementation**: Added `domain_context: Option<&str>` to all prompt builders:
  - `build_unified_chat_prompt_with_domain` — primary chat prompt
  - `build_analysis_sql_prompt_with_domain` — SQL generation
  - `build_analysis_chat_system_prompt_with_domain` — direct chat
  - `build_analysis_retry_prompt_with_domain` — retry with domain context
  - Original functions become zero-cost wrappers passing `None`
- **Wiring**: `chat_turn`, `analysis_chat`, `generate_analysis_sql` in `lib.rs` now read `active_domain_pack()` and pass domain context
- **Files**: `src-tauri/crates/ai/src/prompts.rs`, `src-tauri/crates/ai/src/lib.rs`, `src-tauri/src/lib.rs`

### TASK-PLAT-03: Column Detection (COMPLETE)
- **Implementation**: `detect_domain_columns()` in `domain_pack.rs` — pure function matching schema columns against rule patterns. `format_domain_column_annotations()` produces prompt-ready text. Wired into `chat_turn` to augment domain context.
- **Files**: `src-tauri/crates/engine/src/domain_pack.rs`, `src-tauri/src/lib.rs`

### TASK-PLAT-04: Frontend Parameterization (COMPLETE)
- **Implementation**: `DomainPackConfig` type in appStore, fetched at startup via `get_domain_pack_config` Tauri command. `ChatCard`, `FileList`, `MapView` read from `domainConfig` instead of hardcoded strings/colors.
- **Files**: `src/lib/appStore.ts`, `src/App.tsx`, `src/components/ChatCard.tsx`, `src/components/FileList.tsx`, `src/components/MapView.tsx`, `src-tauri/src/lib.rs`

### TASK-UW-01: Insurance System Prompt (COMPLETE — via domain pack)
- Insurance terminology, data interpretation rules, analysis workflow suggestions, and result interpretation guidance — all in `DomainPack::insurance_underwriting().system_prompt_extension`
- Activated when `SPATIA_DOMAIN_PACK=insurance_underwriting`

### TASK-UW-02: Insurance Column Detection (COMPLETE — via domain pack)
- 24 column detection rules across 4 categories: financial, COPE, policy, risk
- Detected columns are formatted and injected into the AI prompt alongside domain context

### TASK-UW-03: Risk Layer Data Model (est: 4h, role: senior-engineer)
- **Description**: Define the DuckDB schema and ingestion path for risk/hazard overlay datasets. These are the datasets that will eventually be sold via subscription. Initial layers: wildfire risk zones, FEMA flood zones, wind speed contours.
- **Schema design**:
  - `risk_layers` metadata table: layer_name, layer_type (polygon/raster), source, version, bbox, created_at
  - Each risk layer is a DuckDB table with geometry column + risk attributes
  - Spatial index via DuckDB spatial extension for fast point-in-polygon lookups
- **Acceptance criteria**:
  - Can ingest a GeoJSON/GeoParquet risk layer into DuckDB
  - Risk layers appear in the app as available overlay data
  - Point-in-polygon query: "for each property, which flood zone is it in?"
  - Risk layers listed separately from user data tables in UI
- **Files**: `src-tauri/crates/engine/src/risk_layers.rs` (new), `src-tauri/src/lib.rs`, `src/components/FileList.tsx`
- **Dependencies**: None

### TASK-UW-04: Hazard Proximity Analysis Commands (est: 4h, role: senior-engineer)
- **Description**: Add spatial analysis functions that underwriters commonly need: distance-to-hazard, buffer zone analysis, and exposure aggregation within a radius.
- **New engine functions**:
  - `distance_to_nearest(table, risk_layer)` — adds distance column from each point to nearest hazard feature
  - `points_in_zone(table, risk_layer)` — enriches each point with the risk zone it falls within
  - `aggregate_exposure(table, center_lat, center_lon, radius_miles, value_column)` — sums TIV/premium within radius
- **Acceptance criteria**:
  - AI agent can invoke these via SQL or direct commands
  - Results render on map (buffer circles, color-coded risk zones)
  - Performance: <2s for 50K properties against a risk layer
- **Files**: `src-tauri/crates/engine/src/spatial_analysis.rs` (new), `src-tauri/src/lib.rs`
- **Dependencies**: TASK-UW-03

---

## Phase 2: BYOK & Data Subscription Infrastructure (Priority: HIGH)

Goal: Enable the data-as-product monetization model.

### TASK-SUB-01: BYOK API Key Management UI (est: 3h, role: senior-engineer)
- **Description**: Replace environment variable API key configuration with an in-app settings panel. Users bring their own Gemini API key (BYOK model). Keys stored locally in OS keychain via Tauri's secure storage.
- **Acceptance criteria**:
  - Settings panel accessible from app header
  - Gemini API key input with validation (test call)
  - Geocodio API key input (optional)
  - Keys persisted securely (not in plaintext config files)
  - Existing env var approach still works as fallback
- **Files**: `src/components/Settings.tsx` (new), `src-tauri/src/lib.rs`, `src/App.tsx`
- **Dependencies**: None

### TASK-SUB-02: Data Subscription Manifest & Loader (est: 4h, role: senior-engineer)
- **Description**: Build the client-side infrastructure for downloading and managing risk data layers from a subscription service. The actual subscription server is out of scope — this is the client that consumes it.
- **Design**:
  - Manifest file (JSON) describes available layers: name, version, bbox, size, last_updated
  - Loader downloads GeoParquet files from a configured endpoint
  - Local cache in app data directory with version tracking
  - UI shows available vs downloaded layers with update indicators
- **Acceptance criteria**:
  - App can fetch manifest from a configured URL
  - Can download and ingest a risk layer from the manifest
  - Shows download progress
  - Detects when a newer version is available
- **Files**: `src-tauri/crates/engine/src/data_subscription.rs` (new), `src/components/DataCatalog.tsx` (new)
- **Dependencies**: TASK-UW-03

### TASK-SUB-03: Offline Grace Period & License Check (est: 3h, role: senior-engineer)
- **Description**: Implement a lightweight license validation that allows offline usage with a grace period. On startup, app checks a license server. If it can't reach the server, it allows usage for N days (configurable, default 7) based on the last successful check timestamp.
- **Acceptance criteria**:
  - Startup license check against configurable endpoint
  - Cached last-valid timestamp in secure local storage
  - Grace period countdown shown in UI when offline
  - After grace period, risk layers locked but own data still accessible
  - No DRM or obfuscation — straightforward check
- **Files**: `src-tauri/src/license.rs` (new), `src-tauri/src/lib.rs`, `src/App.tsx`
- **Dependencies**: None

---

## Phase 3: Underwriting Workflows (Priority: MEDIUM)

Goal: Build the workflows that make underwriters choose Spatia over spreadsheets.

### TASK-WF-01: Portfolio Concentration Analysis (est: 4h, role: senior-engineer)
- **Description**: One-click analysis that shows where portfolio risk is concentrated geographically. Generates hexbin aggregation of TIV, highlights clusters exceeding threshold, and calculates PML scenarios.
- **Acceptance criteria**:
  - "Analyze concentration" action available when a table with TIV + coordinates is loaded
  - Hexbin map showing aggregated TIV with color gradient
  - Top-10 concentration zones listed with total TIV
  - Configurable threshold for "high concentration" alert
- **Dependencies**: TASK-UW-01, TASK-UW-04

### TASK-WF-02: Single-Risk Assessment Report (est: 4h, role: senior-engineer)
- **Description**: For a single property, generate a comprehensive risk assessment by querying all available risk layers. Output: distance to nearest hazard for each layer, zone classification, comparable properties within radius, and AI-generated risk narrative.
- **Acceptance criteria**:
  - Click a point on map → "Assess Risk" action
  - Report panel shows all risk layer results for that point
  - AI generates a natural-language risk summary
  - Export to PDF (stretch goal)
- **Dependencies**: TASK-UW-01, TASK-UW-03, TASK-UW-04

### TASK-WF-03: Batch Enrichment Pipeline (est: 3h, role: senior-engineer)
- **Description**: Enrich an entire portfolio table with risk scores from all available layers in one operation. Adds columns for each risk layer (flood_zone, wildfire_score, wind_speed, distance_to_coast, etc.).
- **Acceptance criteria**:
  - "Enrich with risk data" action on any geocoded table
  - Progress bar for enrichment (can be slow for large portfolios)
  - New columns added to the DuckDB table
  - Results immediately available for AI chat analysis
- **Dependencies**: TASK-UW-03, TASK-UW-04

---

## Phase 4: Polish & Go-to-Market (Priority: LOW for now)

### TASK-GTM-01: Branded onboarding flow for underwriters
### TASK-GTM-02: Sample dataset bundle (demo portfolio + risk layers)
### TASK-GTM-03: Export capabilities (CSV with enrichment, PDF reports)
### TASK-GTM-04: Performance optimization for 100K+ property portfolios
### TASK-GTM-05: Windows build + code signing + auto-updater

---

## Underwriter Domain Expert (IMPLEMENTED via Domain Pack)

### How it works
The insurance underwriting domain pack injects domain expertise into the existing chat_turn pipeline via `system_prompt_extension`. No separate model or service — same Gemini call, richer context.

### Activation
Set `SPATIA_DOMAIN_PACK=insurance_underwriting` at startup. When active:
- System prompt includes insurance terminology, data interpretation rules, and analysis suggestions
- Column detection identifies financial, COPE, policy, and risk columns in user data
- Detected columns are annotated in the prompt (e.g., "tiv -> Total Insured Value")
- UI text, colors, and map defaults are customized for underwriting

### Example Interactions
- User uploads `book_of_business.csv` → AI sees detected columns (tiv, construction_type, flood_zone) and offers domain-relevant analysis
- User: "Show me my exposure in Florida" → AI generates SQL with underwriting-aware interpretation
- User: "What's my PML here?" → AI uses domain context to explain probable maximum loss scenarios

---

## Architecture Changes Summary

```
Platform:     CSV → Clean → Geocode → Chat (domain-aware) → Map
Domain Pack:  system prompt + column detection + UI config (injected at startup)
Selection:    SPATIA_DOMAIN_PACK env var → DomainPack::from_env() → OnceLock
```

### Platform + Domain Pack Architecture (IMPLEMENTED)
- `src-tauri/crates/engine/src/domain_pack.rs` — DomainPack struct, detection, formatting, generic + insurance constructors
- All prompt builders accept `domain_context: Option<&str>` — zero-cost when None
- Frontend reads `DomainPackConfig` from Tauri at startup, falls back to generic defaults
- Domain pack is immutable for app lifetime (OnceLock)

### New/Planned Rust Modules
- `src-tauri/crates/engine/src/risk_layers.rs` — Risk layer ingestion and management (Phase 2)
- `src-tauri/crates/engine/src/spatial_analysis.rs` — Distance, buffer, aggregation functions (Phase 2)
- `src-tauri/crates/engine/src/data_subscription.rs` — Manifest + download client (Phase 3)
- `src-tauri/src/license.rs` — License check + offline grace period (Phase 3)

### New/Planned Frontend Components
- `src/components/Settings.tsx` — BYOK key management (Phase 3)
- `src/components/DataCatalog.tsx` — Subscription data layer browser (Phase 3)

---

## Team Assignments (Next Phase)

| Agent | Role | Primary Tasks |
|-------|------|---------------|
| senior-engineer | Implementation | TASK-UW-02/03/04, TASK-SUB-01/02/03, TASK-WF-01/02/03 |
| gis-tech-lead | Architecture + coordination | TASK-UW-01, architecture review, sprint planning |
| underwriter-expert (NEW) | Domain consultation | Advisory on all UW tasks, prompt design, workflow validation |
| test-engineer | TDD + integration tests | Test plans for each phase, E2E coverage |
| ui-design-architect | Component design | Settings panel, DataCatalog, risk assessment report |
| product-manager | Scoping + acceptance | User story refinement, acceptance criteria verification |
| gis-domain-expert | Spatial analysis design | Advisory on TASK-UW-03/04, CRS/projection concerns |

---

## Success Metrics

1. **Underwriter can upload a portfolio CSV and see concentration risk on map in <5 minutes**
2. **Risk layer enrichment completes for 10K properties in <30 seconds**
3. **Chat correctly interprets insurance terminology in 90%+ of queries**
4. **Zero proprietary data leaves the user's machine**
5. **App works offline for 7 days after last license check**

---

## Handoff Notes for Tech Lead

### Completed (this sprint):
1. **Platform + Domain Pack architecture** — DomainPack struct, prompt threading, column detection, frontend parameterization
2. **Insurance Underwriting domain pack** — system prompt, 24 column detection rules, UI customization
3. **All prompt builders** accept optional domain context with zero behavioral change when None

### Immediate priorities (next sprint):
1. **TASK-UW-03** — Risk layer data model is foundational for everything in Phase 2+3
2. **TASK-UW-04** — Hazard proximity analysis commands (depends on UW-03)
3. **TASK-SUB-01** — BYOK API key management UI

### How to activate insurance mode:
```bash
SPATIA_DOMAIN_PACK=insurance_underwriting pnpm tauri dev
```

### How to add a new domain pack:
1. Add a new constructor to `DomainPack` in `domain_pack.rs` (e.g., `DomainPack::commercial_real_estate()`)
2. Add the match arm in `DomainPack::from_env()`
3. Define: system prompt extension, column detection rules, UI config
4. That's it — the platform threading is already in place

### Key decisions needed:
- Risk layer file format: GeoParquet (recommended for DuckDB) vs GeoJSON vs PMTiles
- Subscription server stack (out of scope for desktop app, but influences client design)
- License check endpoint design (simple JWT validation recommended)
- Whether to support multiple AI providers beyond Gemini (OpenAI, Claude) for BYOK flexibility
- Whether to support runtime domain pack switching (currently startup-only via OnceLock)

### What NOT to change:
- Core DuckDB architecture — it's working well
- MapLibre + Deck.gl rendering stack — proven and performant
- Tauri v2 shell — stable, no reason to migrate
- SQL safety system — the identifier validation + blocklist approach is sound
- Domain pack abstraction — it's intentionally simple (no plugin system, no dynamic loading)

### Quality gate remains:
```bash
pnpm build
cd src-tauri && cargo test --workspace && cargo clippy --workspace
```
