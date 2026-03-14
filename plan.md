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

## Phase 1: Underwriting Domain Foundation (Priority: CRITICAL)

Goal: Transform the generic GIS chat into an underwriting-aware analysis platform.

### TASK-UW-01: Underwriter AI Agent — System Prompt & Persona (est: 4h, role: gis-tech-lead)
- **Description**: Create a new AI agent persona: the **Underwriter Domain Expert**. This agent understands insurance terminology (COPE, PML, TIV, loss ratios, risk scores, exposure aggregation), knows how to interpret spatial risk data, and can guide analysis from an underwriting perspective. It serves as an always-available industry expert inside the chat.
- **Implementation approach**:
  - New prompt template in `spatia_ai` crate: `underwriter_system_prompt`
  - Injected alongside the existing schema-aware system prompt
  - Domain knowledge includes: COPE scoring, construction types, occupancy classes, protection classes, distance-to-hazard analysis, aggregation/accumulation zones, treaty/facultative boundaries
  - The agent should be able to:
    - Interpret user data in underwriting context ("this column looks like TIV — Total Insured Value")
    - Suggest relevant analyses ("you should check concentration risk within a 1-mile radius")
    - Explain results in underwriting terms ("this cluster represents a PML scenario")
    - Flag data quality issues relevant to underwriting ("missing construction type on 23% of records")
- **Acceptance criteria**:
  - Chat responses demonstrate underwriting domain knowledge
  - Agent correctly identifies common insurance data columns
  - Agent suggests domain-appropriate spatial analyses
  - Non-underwriting queries still work (agent doesn't force insurance context)
- **Files**: `src-tauri/crates/ai/src/prompts.rs` (new prompt), `src-tauri/crates/ai/src/lib.rs` (expose), `src-tauri/src/lib.rs` (wire into chat_turn)
- **Dependencies**: None

### TASK-UW-02: Insurance Data Column Recognition (est: 3h, role: senior-engineer)
- **Description**: Extend the schema detection to recognize common insurance/underwriting columns beyond just addresses. This helps the AI agent provide better context and the app can offer smarter defaults.
- **Recognized patterns**:
  - **Financial**: TIV, total_insured_value, premium, deductible, limit, retention, loss, paid_loss, incurred_loss
  - **COPE**: construction_type, occupancy, protection_class, external_exposure, year_built, stories, sq_ft, roof_type
  - **Policy**: policy_number, policy_id, effective_date, expiration_date, line_of_business, coverage_type
  - **Location**: latitude, longitude, address, city, state, zip, county, country, geocode_quality
  - **Risk**: risk_score, hazard_score, flood_zone, wildfire_risk, wind_pool, earthquake_zone, distance_to_coast
- **Acceptance criteria**:
  - New `detect_insurance_columns` function in engine
  - Returns categorized column mapping (financial, cope, policy, location, risk)
  - Integrated into chat system prompt so AI has column context
- **Files**: `src-tauri/crates/engine/src/schema.rs`, `src-tauri/crates/ai/src/prompts.rs`
- **Dependencies**: None

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

## New AI Agent: Underwriter Domain Expert

### Purpose
An always-available insurance industry expert embedded in the chat. Not a separate UI — it enriches the existing chat_turn with domain knowledge.

### Capabilities
1. **Data Interpretation**: Recognizes insurance data patterns, explains what columns mean, flags data quality issues relevant to underwriting
2. **Analysis Guidance**: Suggests appropriate spatial analyses based on underwriting context (concentration, proximity, accumulation)
3. **Result Explanation**: Translates analytical results into underwriting language (PML, risk appetite, treaty implications)
4. **Regulatory Awareness**: Knows about admitted vs surplus lines, state-specific rules, catastrophe modeling concepts
5. **Workflow Optimization**: Recommends next steps in the underwriting process based on current data state

### Implementation
- Implemented as an enhanced system prompt in `spatia_ai`, not a separate model or service
- Activated contextually when insurance-related data is detected (via TASK-UW-02)
- Falls back to general GIS analysis mode for non-insurance data
- Uses the same Gemini model — no additional API cost

### Example Interactions
- User uploads `book_of_business.csv` → Agent: "I see TIV, construction type, and year built columns. This looks like a property portfolio. Want me to check concentration risk or run a COPE analysis?"
- User: "Show me my exposure in Florida" → Agent generates SQL filtering FL properties, aggregates TIV, renders on map with hurricane zone overlay
- User: "What's my PML here?" → Agent calculates aggregated TIV within catastrophe zones, explains the scenario

---

## Architecture Changes Summary

```
Current:  CSV → Clean → Geocode → Chat (generic GIS) → Map
Pivot:    CSV → Clean → Detect Insurance Columns → Geocode → Enrich with Risk Layers
          → Chat (Underwriter Expert) → Spatial Analysis → Map + Reports
```

### New Rust Modules
- `src-tauri/crates/engine/src/risk_layers.rs` — Risk layer ingestion and management
- `src-tauri/crates/engine/src/spatial_analysis.rs` — Distance, buffer, aggregation functions
- `src-tauri/crates/engine/src/data_subscription.rs` — Manifest + download client
- `src-tauri/src/license.rs` — License check + offline grace period

### New Frontend Components
- `src/components/Settings.tsx` — BYOK key management
- `src/components/DataCatalog.tsx` — Subscription data layer browser

### Modified Files
- `src-tauri/crates/ai/src/prompts.rs` — Underwriter system prompt
- `src-tauri/crates/engine/src/schema.rs` — Insurance column detection
- `src-tauri/src/lib.rs` — New Tauri commands
- `src/components/FileList.tsx` — Risk layer separation
- `src/components/ChatCard.tsx` — Enhanced context display

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

### Immediate priorities (this sprint):
1. **TASK-UW-01** — The underwriter system prompt is the highest-leverage task. It transforms every chat interaction. Start here.
2. **TASK-UW-02** — Insurance column detection feeds into the prompt. Do this alongside UW-01.
3. **TASK-UW-03** — Risk layer data model is foundational for everything in Phase 2+3.

### Key decisions needed:
- Risk layer file format: GeoParquet (recommended for DuckDB) vs GeoJSON vs PMTiles
- Subscription server stack (out of scope for desktop app, but influences client design)
- License check endpoint design (simple JWT validation recommended)
- Whether to support multiple AI providers beyond Gemini (OpenAI, Claude) for BYOK flexibility

### What NOT to change:
- Core DuckDB architecture — it's working well
- MapLibre + Deck.gl rendering stack — proven and performant
- Tauri v2 shell — stable, no reason to migrate
- SQL safety system — the identifier validation + blocklist approach is sound

### Quality gate remains:
```bash
pnpm build
cd src-tauri && cargo test --workspace && cargo clippy --workspace
```
