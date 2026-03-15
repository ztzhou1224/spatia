# Spatia Pivot Plan — BYOK AI-Native Desktop App for Insurance Underwriters

## Active Sprint: Free Geocoding Pipeline (Nominatim)

**Sprint date:** 2026-03-15
**Goal:** Replace Geocodio with OSM Nominatim as the HTTP geocoding fallback, making Spatia's geocoding pipeline completely free for production use. Geocodio is retained for testing/benchmarking only.

---

## Tech Spec: Free Geocoding Pipeline (Overture + Nominatim)

### Problem Statement

Spatia's geocoding pipeline currently depends on Geocodio as the HTTP fallback geocoder when Overture local matching fails. Geocodio is a paid API ($0.50/1000 lookups), creating a cost barrier for users. The goal is to replace Geocodio with OSM Nominatim (free, rate-limited to 1 req/sec) as the production fallback, while keeping Geocodio available behind a feature gate for accuracy benchmarking.

Key constraints:
- Nominatim enforces a strict 1 request/second rate limit (usage policy)
- Must provide clear UX feedback: estimated time, per-address progress
- Must support background processing so users can interact with partially-geocoded data
- Geocodio code must remain compilable and testable but not be part of the default production flow

### Current Architecture (as-is)

```
geocode_batch_with_components()
  |
  +-- if SPATIA_GEOCODIO_API_KEY set && batch <= 500:
  |     geocode_batch_api_first()    # Cache -> Overture existing -> Geocodio API -> GERS lookup
  |
  +-- else:
        geocode_batch_overture_first()  # Cache -> Overture S3 download -> Overture exact/fuzzy -> local fuzzy -> Geocodio API -> GERS lookup
```

Both paths end with Geocodio as the final fallback. The Geocodio client (`geocodio.rs`) sends batch POST requests to `/v1.10/geocode`.

**Key files:**
- `src-tauri/crates/geocode/src/geocode.rs` — orchestration (`geocode_batch_*` functions)
- `src-tauri/crates/geocode/src/geocodio.rs` — Geocodio HTTP client
- `src-tauri/crates/geocode/src/cache.rs` — `geocode_cache` table (DuckDB)
- `src-tauri/crates/geocode/src/overture_cache.rs` — Overture address cache + exact/fuzzy matching
- `src-tauri/crates/geocode/src/types.rs` — `GeocodeResult`, `GeocodeBatchResult`, `GeocodeStats`
- `src-tauri/crates/geocode/src/lib.rs` — public re-exports
- `src-tauri/crates/geocode/Cargo.toml` — dependencies
- `src-tauri/src/lib.rs` — `geocode_table_column` Tauri command (lines 276-497)
- `src/components/FileList.tsx` — geocoding UI (handleGeocode, GeocodeStatsSummary)
- `src/lib/appStore.ts` — `GeocodeStats` type, `TableInfo.geocodeStats`

### Proposed Approach

#### 1. Nominatim Client Module (`nominatim.rs`)

New module in `spatia_geocode` crate. Uses Nominatim's `/search` endpoint (not batch -- Nominatim has no batch endpoint). Each address is a separate HTTP GET request.

**Rate limiter:** `tokio::time::sleep(Duration::from_secs(1))` between requests. This is the simplest correct approach. No need for a token bucket -- Nominatim's policy is literally "max 1 req/sec."

**API contract:**
```rust
pub(crate) struct NominatimResult {
    pub(crate) inner: GeocodeResult,
    pub(crate) importance: f64,  // Nominatim's importance score [0,1]
}

pub(crate) async fn geocode_via_nominatim(
    addresses: &[String],
    progress: impl Fn(usize, usize),  // (completed, total) callback
) -> GeoResult<Vec<NominatimResult>>;
```

**Key design decisions:**
- User-Agent header MUST identify the app: `Spatia/0.1 (https://spatia.dev)` (Nominatim TOS requirement)
- Use `format=jsonv2` for structured results
- Use `countrycodes=us` default (configurable via `SPATIA_NOMINATIM_COUNTRY` env var)
- No API key required; base URL defaults to `https://nominatim.openstreetmap.org` but overridable via `SPATIA_NOMINATIM_BASE_URL` for self-hosted instances (which have no rate limit)

#### 2. Feature-Gate Geocodio (`geocodio` feature)

Move Geocodio from default production code to a Cargo feature gate:
- Add `geocodio` feature to `spatia_geocode/Cargo.toml` (NOT default)
- Wrap `geocodio.rs` module with `#[cfg(feature = "geocodio")]`
- Wrap Geocodio-dependent code in `geocode.rs` with `#[cfg(feature = "geocodio")]`
- Benchmark crate (`spatia_bench`) enables the `geocodio` feature for accuracy comparison

#### 3. Pipeline Restructuring

New production flow (default, no feature flags):
```
geocode_batch_with_components()
  |
  +-- Cache lookup (unchanged)
  +-- Overture exact match (unchanged)
  +-- Overture fuzzy match (unchanged)
  +-- Local fuzzy geocode via lookup tables (unchanged)
  +-- Nominatim HTTP fallback (NEW -- 1 req/sec, with progress callback)
  +-- GERS reverse lookup (unchanged)
```

The strategy selection logic changes:
- **Before:** `if has_geocodio_key && batch <= 500 -> api_first; else -> overture_first`
- **After:** Always use Overture-first pipeline. The "api_first" fast path is removed from default builds (it was a Geocodio optimization). Nominatim's 1-req/sec rate limit means batch API shortcuts don't apply.

When `geocodio` feature is enabled AND `SPATIA_GEOCODIO_API_KEY` is set, the old Geocodio paths remain available for benchmarking via explicit function calls.

#### 4. Background Geocoding with Progressive Results

This is the most significant architectural change. Currently, `geocode_table_column` is a single async Tauri command that blocks until all geocoding completes. With Nominatim's 1 req/sec limit, 100 addresses = ~100 seconds of blocking.

**New approach:** Split geocoding into two phases:
1. **Fast phase** (< 2 sec): Cache + Overture local matching. Write results to DuckDB immediately. Return partial results + count of remaining addresses.
2. **Background phase** (async): Nominatim HTTP fallback for unresolved addresses. Each result is written to DuckDB as it arrives. Emit Tauri events per-address so the frontend can update progress.

**Implementation:**
- The existing `geocode_table_column` Tauri command runs the fast phase and returns immediately with partial results.
- It spawns a `tokio::task` for the Nominatim background phase.
- The background task emits `geocode-progress` events with: `{ stage: "nominatim", message: "Geocoding address 14/87 via Nominatim...", percent, eta_seconds, current_address }`.
- Each Nominatim result is written to the DuckDB table incrementally (UPDATE per row).
- When complete, emits a final `geocode-progress` event with `stage: "completed"`.
- The frontend shows a progress bar with ETA: "Geocoding remaining 87 addresses (~87 seconds)".
- Users can interact with the map/data using already-geocoded results while the background task runs.

**Cancellation:** Add a Tauri command `cancel_geocode` that sets an `AtomicBool` flag checked by the background task between Nominatim requests.

#### 5. Frontend Changes

- **FileList.tsx:** After initial geocode returns, show a background progress indicator if Nominatim addresses remain. Display "X of Y addresses geocoded. Remaining ~Ns via Nominatim." with a cancel button.
- **appStore.ts:** Add `geocodeInProgress: boolean` and `geocodeBackgroundProgress: { completed: number, total: number, eta: number } | null` to store state.
- **GeocodeStatsSummary:** Update to show "nominatim" as a source instead of "geocodio".

#### 6. Stats & Source Tracking

- `GeocodeStats.api_resolved` renamed to `nominatim_resolved` in production (breaking change, but stats are internal).
- Source string in `GeocodeResult.source` changes from `"geocodio"` to `"nominatim"`.
- `_geocode_source` column values in DuckDB tables will show `"nominatim"`.
- `default_confidence("nominatim")` set to 0.80 (Nominatim is generally slightly less precise than Geocodio for US addresses).

### Tasks

1. **[TASK-NOM-01] Nominatim HTTP client module** (est: 3h, role: engine, agent: senior-engineer)
   - Create `src-tauri/crates/geocode/src/nominatim.rs`
   - Implement `geocode_via_nominatim()` with:
     - Per-request 1-second rate limiting via `tokio::time::sleep`
     - Proper User-Agent header (`Spatia/0.1`)
     - JSON response parsing (Nominatim `format=jsonv2`)
     - Progress callback `Fn(usize, usize)` for (completed, total)
     - `importance` score mapping to confidence
     - Base URL from `SPATIA_NOMINATIM_BASE_URL` env var (default: `https://nominatim.openstreetmap.org`)
     - Country filter from `SPATIA_NOMINATIM_COUNTRY` env var (default: `us`)
   - Add `mod nominatim;` to `lib.rs`
   - Unit tests with mockito (same pattern as geocodio.rs tests):
     - TC-NOM-01: Single address returns correct lat/lon
     - TC-NOM-02: No results returns empty vec (not error)
     - TC-NOM-03: HTTP 429 (rate limited) returns error
     - TC-NOM-04: Malformed JSON returns error
     - TC-NOM-05: Empty input returns empty without HTTP call
   - Acceptance criteria: `cargo test -p spatia_geocode` passes with new tests
   - Dependencies: none

2. **[TASK-NOM-02] Feature-gate Geocodio** (est: 2h, role: engine, agent: senior-engineer)
   - Add `geocodio = []` feature to `src-tauri/crates/geocode/Cargo.toml`
   - Wrap `mod geocodio` with `#[cfg(feature = "geocodio")]`
   - Wrap all Geocodio imports/usage in `geocode.rs` with `#[cfg(feature = "geocodio")]`
   - Wrap `geocode_via_geocodio` re-export in `lib.rs` with `#[cfg(feature = "geocodio")]`
   - In `geocode_batch_api_first`: gate behind `#[cfg(feature = "geocodio")]`, add `#[cfg(not(feature = "geocodio"))]` stub that returns error
   - Update `src-tauri/crates/bench/Cargo.toml` to depend on `spatia_geocode` with `features = ["geocodio"]`
   - Ensure `cargo test -p spatia_geocode` passes WITHOUT the geocodio feature (default)
   - Ensure `cargo test -p spatia_geocode --features geocodio` passes WITH the feature
   - Acceptance criteria: Default build has zero Geocodio code compiled; bench crate still compiles with geocodio
   - Dependencies: none (can parallelize with TASK-NOM-01)

3. **[TASK-NOM-03] Integrate Nominatim into geocode pipeline** (est: 4h, role: engine, agent: senior-engineer)
   - Modify `geocode_batch_overture_first()` in `geocode.rs`:
     - Replace the Geocodio fallback block (currently lines 789-860) with Nominatim call
     - Use `geocode_via_nominatim()` for unresolved addresses
     - Map `NominatimResult.importance` to confidence (use as-is if > 0, else `default_confidence("nominatim")`)
     - Cache Nominatim results via existing `cache_store()` with source `"nominatim"`
     - GERS reverse lookup still applies to Nominatim results
   - Modify `geocode_batch_with_components()`:
     - Remove the `has_api_key && batch <= limit` fast-path branch (or gate it behind `#[cfg(feature = "geocodio")]`)
     - Default path is always Overture-first with Nominatim fallback
   - Update `default_confidence()` to handle `"nominatim"` (return 0.80)
   - Update `GeocodeStats`: rename `api_resolved` field to `nominatim_resolved` (or keep as `api_resolved` and just change the source label -- evaluate breakage)
   - Acceptance criteria: `geocode_batch()` resolves addresses without any API key set (using Nominatim); existing cache and Overture paths unchanged
   - Dependencies: TASK-NOM-01, TASK-NOM-02

4. **[TASK-NOM-04] Background geocoding with progressive results** (est: 5h, role: fullstack, agent: senior-engineer)
   - **Rust side (`src-tauri/src/lib.rs`):**
     - Refactor `geocode_table_column` to split into fast phase + background phase
     - Fast phase: cache + Overture matching, write partial results to DuckDB, return immediately with `{ status: "partial", geocoded_count, remaining_count, estimated_seconds }`
     - Background phase: spawn `tokio::task::spawn` with Nominatim geocoding
     - Add `static GEOCODE_CANCEL: AtomicBool` flag (or per-table cancel map)
     - Background task: for each Nominatim result, UPDATE the DuckDB row immediately, emit `geocode-progress` event
     - New Tauri command: `cancel_geocode(table_name: String)` to set cancel flag
     - Progress events: `{ stage: "nominatim", completed: N, total: M, eta_seconds: S, current_address: "..." }`
   - **Frontend side:**
     - `src/lib/appStore.ts`: Add `geocodeBackgroundProgress` state
     - `src/components/FileList.tsx`:
       - Listen for `geocode-progress` events with `stage: "nominatim"`
       - Show inline progress bar: "Geocoding via Nominatim: 14/87 (~73s remaining)"
       - Add cancel button that calls `cancel_geocode`
       - When background completes, refresh table stats
     - Table data should be usable while background geocoding runs (already-geocoded rows have _lat/_lon filled)
   - Acceptance criteria:
     - User sees partial results on map immediately after fast phase
     - Background progress is visible in FileList
     - Cancel stops the background task within 1 second
     - All results are written to DuckDB incrementally (not batched at end)
   - Dependencies: TASK-NOM-03

5. **[TASK-NOM-05] Frontend progress UX and stats update** (est: 2h, role: frontend, agent: senior-engineer)
   - Update `GeocodeStatsSummary` component to show "nominatim" source instead of "geocodio"
   - Add ETA display before geocoding starts: "100 addresses: ~12s local + ~88s Nominatim"
     - Pre-compute estimate: local phase ~2s, Nominatim = (remaining * 1 second)
   - Update `GeocodeStats` type in `appStore.ts` to include `nominatim` in by_source
   - Show time elapsed during background geocoding
   - Acceptance criteria: Stats display is accurate, ETA is within 20% of actual time, no reference to "geocodio" in production UI
   - Dependencies: TASK-NOM-04

6. **[TASK-NOM-06] Integration tests** (est: 3h, role: engine, agent: test-engineer)
   - Add integration test in `spatia_geocode`:
     - Test full pipeline with mockito Nominatim server: cache miss -> Overture miss -> Nominatim -> cached
     - Test rate limiting: verify at least 1 second between requests (measure elapsed time for 3 requests >= 2 seconds)
     - Test cancellation: start geocoding, cancel after 2nd request, verify only 2 results
     - Test self-hosted URL override via `SPATIA_NOMINATIM_BASE_URL`
   - Add integration test for feature-gated Geocodio:
     - `#[cfg(feature = "geocodio")]` test that Geocodio path still works
   - Acceptance criteria: All tests pass in CI; `cargo test -p spatia_geocode` and `cargo test -p spatia_geocode --features geocodio`
   - Dependencies: TASK-NOM-03

7. **[TASK-NOM-07] Documentation and env var updates** (est: 1h, role: fullstack, agent: senior-engineer)
   - Update `CLAUDE.md`:
     - Add `SPATIA_NOMINATIM_BASE_URL` and `SPATIA_NOMINATIM_COUNTRY` to env vars section
     - Note that `SPATIA_GEOCODIO_API_KEY` is now only used with `geocodio` feature flag
     - Update geocoding flow description
   - Update `summary.md` if it references Geocodio as production dependency
   - Update Settings UI (`SettingsPanel.tsx`): remove Geocodio API key input from production UI (or label it "Benchmarking only")
   - Acceptance criteria: All docs accurate, no orphan references to Geocodio as required dependency
   - Dependencies: TASK-NOM-04, TASK-NOM-05

### Sequencing & Parallelization

```
TASK-NOM-01 ──┐
              ├──> TASK-NOM-03 ──> TASK-NOM-04 ──> TASK-NOM-05
TASK-NOM-02 ──┘                        |               |
                                       v               v
                                  TASK-NOM-06      TASK-NOM-07
```

- NOM-01 and NOM-02 can run in parallel (no dependencies)
- NOM-03 requires both NOM-01 and NOM-02
- NOM-04 requires NOM-03 (needs the integrated pipeline)
- NOM-05 and NOM-06 can start once NOM-04 is done
- NOM-07 is last (needs everything settled)

**Total estimated effort:** 20 hours across 7 tasks

### Risks & Open Questions

1. **Nominatim accuracy vs Geocodio:** Nominatim may return less precise results for US addresses, especially in suburban/rural areas. Mitigation: the Overture local matching handles the majority of addresses; Nominatim is only for the long tail. We should run a benchmark comparing accuracy before fully deprecating Geocodio access.

2. **Rate limit at scale:** 500 addresses = ~8 minutes of Nominatim-only geocoding. For large datasets, this is slow. Mitigations:
   - Overture local matching resolves 60-80% of US addresses, reducing the Nominatim load
   - Self-hosted Nominatim has no rate limit (power users can set `SPATIA_NOMINATIM_BASE_URL`)
   - Background processing means the user is not blocked

3. **Nominatim TOS compliance:** Must include proper User-Agent, respect rate limits, and not bulk-geocode for commercial redistribution. Spatia's use case (user's own data, desktop app) is compliant, but worth documenting.

4. **AtomicBool cancel pattern:** If multiple tables are geocoded simultaneously, a single static `AtomicBool` is insufficient. Consider using a `DashMap<String, Arc<AtomicBool>>` keyed by table name. For MVP, a single-table-at-a-time constraint is acceptable.

5. **GeocodeStats field rename:** Renaming `api_resolved` to `nominatim_resolved` is a breaking change for any code that reads stats. Since stats are internal (not persisted), this is low-risk. Alternative: keep the field name as `api_resolved` and only change the source label string.

6. **Geocodio feature gate in engine crate:** The `spatia_engine` crate re-exports `geocode_via_geocodio`. This re-export and any engine-level code that references Geocodio must also be feature-gated. Check `src-tauri/crates/engine/src/lib.rs` re-exports.

7. **Self-hosted Nominatim for power users:** Document that users who need faster geocoding can run their own Nominatim instance (Docker one-liner) and point `SPATIA_NOMINATIM_BASE_URL` at it. This eliminates the rate limit entirely.

### Quality Gate

```bash
pnpm build                                              # Frontend builds clean
cd src-tauri && cargo test --workspace                   # All tests pass (default features)
cd src-tauri && cargo test --workspace --features geocodio  # Geocodio tests still pass
cd src-tauri && cargo clippy --workspace                 # No warnings
```

### Architecture Decision Record

**Decision:** Use Nominatim as HTTP fallback instead of Geocodio for production geocoding.

**Rationale:**
- Zero cost for users (Nominatim is free for low-volume use)
- Aligns with Spatia's local-first philosophy (can self-host Nominatim)
- Geocodio's batch API advantage is negated by Overture local matching (which resolves most addresses before HTTP fallback)
- Rate limit (1 req/sec) is acceptable because: (a) Overture handles 60-80% of addresses locally, (b) background processing prevents blocking, (c) self-hosted option exists for power users

**Tradeoffs accepted:**
- Slower than Geocodio for HTTP fallback (1 req/sec vs batch)
- Potentially lower accuracy for some US addresses
- More complex UX (background progress, ETA display)

**Rejected alternatives:**
- Pelias (free, self-hosted only -- too much setup for casual users)
- Photon (Komoot's geocoder -- smaller community, less complete data)
- Keep Geocodio as default (contradicts "free geocoding" goal)

---

## Previous Sprint: Table Stakes (Phase 1) — COMPLETE

**Sprint date:** 2026-03-14
**Goal:** Ship all 8 pre-launch blocker features to make Spatia launch-ready.

### Completed Tasks

- [x] TASK-14: CSV export of any table — `export.rs` engine module + Tauri command + FileList download button
- [x] TASK-15: GeoJSON export of analysis_result — `export_analysis_geojson` engine + Tauri command + ChatCard export button
- [x] TASK-16: Map PNG export — `save_file` Tauri command + canvas compositing (MapLibre + Deck.gl) + `preserveDrawingBuffer`
- [x] TASK-17: Settings UI — `tauri-plugin-store` + `SettingsPanel.tsx` modal + `save/get/delete_api_key` commands + env var injection at startup
- [x] TASK-18: Map legend — `MapLegend.tsx` component (scatter/heatmap/hexbin variants) + positioned overlay
- [x] TASK-19: Basemap selector — `BasemapSelector.tsx` (Dark/Light/OSM) + `basemapId` in appStore with localStorage persistence + style.load re-apply
- [x] TASK-20: Truncation indicators — `COUNT(*)` before LIMIT in analysis.rs + `total_count` in ChatTurnResult + badges on map and table
- [x] TASK-21: Tooltip labels — All icon-only buttons audited and `title` attributes added

### New Files Created
- `src-tauri/crates/engine/src/export.rs` — CSV and GeoJSON export functions
- `src/components/MapLegend.tsx` — Auto-generated map legend overlay
- `src/components/BasemapSelector.tsx` — Basemap switching control
- `src/components/SettingsPanel.tsx` — API key management modal

### Quality Gate
- `pnpm build` — PASS
- `cargo test -p spatia_engine` — 65/68 pass (3 pre-existing failures due to network-restricted environment)
- `cargo clippy -p spatia_engine` — PASS (no warnings)

---

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
---

# Post-MVP: Insurance Vertical Sprint Plan

**Date:** 2026-03-14
**Context:** Following the market-fit-analysis, Spatia is pivoting from generic desktop GIS to **vertical spatial intelligence for insurance underwriting**. Core technology unchanged; positioning, data integrations, AI prompts, and workflow design now target property risk assessment for small-to-mid insurance carriers and MGAs.

**Strategic rationale:** Insurance underwriting is the strongest vertical because (1) bad risk assessment costs insurers millions, (2) local-first becomes a compliance feature for sensitive policy data, (3) the existing CSV→geocode→analyze→map pipeline maps directly to underwriting workflows, and (4) incumbent tools (SpatialKey/Insurity) cost $100K+/year — Spatia can compete at 1/10th the price.

---

## Phase 1: Table Stakes (Pre-Launch Blockers)

Goal: Ship the minimum capabilities required for any professional user to complete a workflow end-to-end. Without these, Spatia cannot be positioned as a production tool.

### TASK-14: CSV export of any table (est: 3h, role: senior-engineer)
- **Description**: Add a "Download CSV" button to each table card in the FileList panel. Users must be able to export any loaded table (raw, geocoded, or analysis_result) as a CSV file.
- **Approach**: New Tauri command `export_table_csv` that runs `COPY <table> TO '<path>' (FORMAT CSV, HEADER)` via DuckDB. Frontend uses Tauri's save dialog (`dialog.save`) for file path selection.
- **Acceptance criteria**:
  - Each table card in FileList shows a download/export icon button
  - Clicking opens a native save dialog defaulting to `<table_name>.csv`
  - Exported CSV includes headers and all rows
  - Works for regular tables, geocoded tables, and the `analysis_result` view
- **Files**: `src-tauri/src/lib.rs` (new command), `src-tauri/crates/engine/src/export.rs` (new module), `src/components/FileList.tsx` (export button)
- **Dependencies**: None

### TASK-15: GeoJSON export of analysis_result (est: 2h, role: senior-engineer)
- **Description**: Add "Export GeoJSON" button for the current analysis result. This enables users to share spatial outputs with other GIS tools.
- **Approach**: New Tauri command `export_analysis_geojson` that serializes the current `analysis_result` view to GeoJSON FeatureCollection and saves via native dialog.
- **Acceptance criteria**:
  - Export button appears in ChatCard when analysis results exist
  - Exports valid GeoJSON FeatureCollection with all properties
  - File saved via native save dialog defaulting to `analysis_result.geojson`
- **Files**: `src-tauri/src/lib.rs`, `src-tauri/crates/engine/src/export.rs`, `src/components/ChatCard.tsx`
- **Dependencies**: TASK-14 (shared export module)

### TASK-16: Map PNG export (est: 2h, role: senior-engineer)
- **Description**: Add "Export Map" button to MapView toolbar that captures the current map viewport as a PNG image.
- **Approach**: Use `map.getCanvas().toDataURL('image/png')` on the MapLibre instance, then pass the base64 data to a Tauri command that writes it to disk via save dialog.
- **Acceptance criteria**:
  - Export button visible in map toolbar/controls area
  - Captures full viewport including all Deck.gl overlays and base map
  - Saved as PNG via native save dialog
  - Works with all basemap types and layer combinations
- **Files**: `src/components/MapView.tsx`, `src-tauri/src/lib.rs` (save file command)
- **Dependencies**: None

### TASK-17: Settings UI — API key management (est: 4h, role: senior-engineer)
- **Description**: Build a settings panel accessible from the app toolbar. Users must be able to enter, update, and verify API keys (Gemini, Geocodio) without touching environment variables. Keys stored via Tauri's secure storage plugin.
- **Approach**: New `SettingsPanel` component (modal or slide-over). Use `tauri-plugin-store` or `tauri-plugin-stronghold` for secure key storage. New Tauri commands: `save_api_key`, `get_api_key`, `verify_api_key`. At startup, check secure storage before falling back to env vars.
- **Acceptance criteria**:
  - Settings gear icon in the app header/toolbar opens the settings panel
  - Fields for Gemini API key and Geocodio API key (masked input)
  - "Test" button that verifies each key responds (ping the API)
  - Keys persisted across sessions via Tauri secure storage
  - Env vars still work as fallback (backward compatible)
  - PMTiles file picker (native file dialog) to select local tile files
- **Files**: `src/components/SettingsPanel.tsx` (new), `src/App.tsx` (mount settings), `src-tauri/src/lib.rs` (key commands), `src-tauri/Cargo.toml` (secure storage plugin)
- **Dependencies**: None

### TASK-18: Map legend — auto-generated from active layer (est: 3h, role: senior-engineer)
- **Description**: Add an auto-generated legend overlay to MapView that reflects the current active Deck.gl layer type, color encoding, and data source.
- **Approach**: New `MapLegend` component rendered as a positioned overlay inside MapView. Reads `visualizationType`, layer color config, and data source name from appStore. Renders appropriate legend items: color gradient for heatmap, color stops for hexbin, single color for scatter.
- **Acceptance criteria**:
  - Legend appears when any Deck.gl overlay is active
  - Shows layer type name, color scale, and data source table name
  - For quantitative scales (heatmap, hexbin): shows min/max range
  - For scatter: shows point color and label
  - Legend hides when no overlay is active
  - Positioned bottom-left or top-right, non-overlapping with other controls
- **Files**: `src/components/MapLegend.tsx` (new), `src/components/MapView.tsx` (mount legend), `src/lib/appStore.ts` (legend state if needed)
- **Dependencies**: None

### TASK-19: Basemap selector (est: 2h, role: senior-engineer)
- **Description**: Add a basemap selector control to the map. Minimum options: CartoDB Dark Matter, CartoDB Positron (light), and OpenStreetMap.
- **Approach**: New `BasemapSelector` component (small floating button group or dropdown) in MapView. On selection, update the MapLibre style URL. Store selection in appStore for persistence.
- **Acceptance criteria**:
  - Basemap selector visible on the map (floating control)
  - Three options minimum: Dark, Light, OpenStreetMap
  - Switching basemaps preserves current viewport (center, zoom)
  - Preserves all Deck.gl overlays and data layers
  - Selection persists across sessions (localStorage or appStore)
- **Files**: `src/components/BasemapSelector.tsx` (new), `src/components/MapView.tsx` (mount selector), `src/lib/appStore.ts` (basemap state)
- **Dependencies**: None
- **Note**: Already listed in architecture.md as implemented basemaps — verify current state before starting. If partially done, extend rather than rebuild.

### TASK-20: Truncation indicators on map and table (est: 2h, role: senior-engineer)
- **Description**: When results are capped (1,000 GeoJSON features, 20 table rows), show an explicit "Showing X of Y" badge. Silent truncation destroys analytical trust.
- **Approach**: Extend analysis SQL execution to return total row count alongside truncated results (run `SELECT COUNT(*) FROM analysis_result` before truncation). Display badge on map overlay and in ResultTable header.
- **Acceptance criteria**:
  - Map shows "Showing X of Y features" badge when GeoJSON is truncated
  - ResultTable shows "Showing X of Y rows" in header when rows are truncated
  - Badge only appears when truncation actually occurs
  - Total count is accurate (from COUNT(*) query)
- **Files**: `src-tauri/src/lib.rs` (return total count), `src/components/MapView.tsx` (badge), `src/components/ChatCard.tsx` (table badge)
- **Dependencies**: None

### TASK-21: Tooltip labels on all UI controls (est: 2h, role: senior-engineer)
- **Description**: Add descriptive tooltip labels to all icon-only buttons across the UI. Currently, many controls are unlabeled icons that are not discoverable.
- **Approach**: Audit all icon buttons in MapView, FileList, ChatCard, and any other components. Add Radix UI `Tooltip` wrappers with descriptive labels.
- **Acceptance criteria**:
  - Every icon-only button has a hover tooltip describing its function
  - Tooltips use consistent styling (Radix UI Tooltip component)
  - Labels are concise and action-oriented (e.g., "Export CSV", "New Chat", "Toggle Layer")
- **Files**: `src/components/MapView.tsx`, `src/components/FileList.tsx`, `src/components/ChatCard.tsx`
- **Dependencies**: None

---

## Phase 2: Competitive Parity

Goal: Bring Spatia to a level where direct comparison against Felt, Kepler.gl, and lighter ArcGIS/Carto use cases is favorable.

### TASK-22: GeoJSON and Shapefile import (est: 4h, role: senior-engineer)
- **Description**: Extend the ingest pipeline to accept `.geojson` and `.shp` files in addition to CSV. Without polygon data, spatial joins and geographic aggregations are impossible.
- **Approach**: DuckDB spatial extension supports `ST_Read()` for GeoJSON and Shapefile (via GDAL bindings). Extend `ingest_csv_with_progress` to detect file extension and route to appropriate DuckDB load command. Geometry columns stored as DuckDB GEOMETRY type.
- **Acceptance criteria**:
  - FileList upload accepts `.geojson`, `.json`, and `.shp` files (plus `.dbf`/`.shx`/`.prj` sidecar files for Shapefile)
  - Ingested spatial files appear as tables with geometry columns
  - Polygons/lines render on map (not just points)
  - AI analysis can reference geometry columns in SQL
- **Files**: `src-tauri/crates/engine/src/ingest.rs` (extend), `src-tauri/src/lib.rs` (update command), `src/components/FileList.tsx` (accept new file types), `src/components/MapView.tsx` (polygon/line rendering)
- **Dependencies**: None

### TASK-23: Column sort and filter in table preview (est: 3h, role: senior-engineer)
- **Description**: Add column-level sorting (click header to toggle asc/desc) and a row count indicator to the table preview. Phase 2 addition: basic column filter (text search per column).
- **Approach**: Extend the `table_preview` Tauri command to accept optional `order_by` and `filter` parameters. Frontend adds clickable headers and filter input per column.
- **Acceptance criteria**:
  - Clicking a column header sorts by that column (toggle asc → desc → none)
  - Sort state indicated by arrow icon in header
  - Row count indicator shows total rows in table
  - Optional: text filter input per column (WHERE col LIKE '%query%')
- **Files**: `src-tauri/src/lib.rs` (extend preview command), `src/components/FileList.tsx` (sortable headers, filter UI)
- **Dependencies**: None

### TASK-24: Editable SQL panel in chat (est: 3h, role: senior-engineer)
- **Description**: Show the AI-generated SQL in a collapsible panel within each chat response. Allow users to edit and re-execute the SQL. This provides transparency and a power-user escape hatch when AI gets it wrong.
- **Approach**: ChatCard already shows some SQL info. Extend to show full SQL in a collapsible `<pre>` block with an "Edit & Run" button. Edited SQL goes through the existing safety validator before execution.
- **Acceptance criteria**:
  - Each AI response that generated SQL shows a collapsible "View SQL" section
  - SQL is displayed in a monospace, syntax-highlighted text area
  - "Edit" button makes the SQL editable; "Run" button re-executes
  - Edited SQL still passes through the analysis SQL safety validator
  - Results update in the chat message and on the map
- **Files**: `src/components/ChatCard.tsx`, `src/components/SqlEditor.tsx` (new, lightweight)
- **Dependencies**: None

### TASK-25: Example query suggestions in empty chat (est: 2h, role: senior-engineer)
- **Description**: When no conversation is in progress, show clickable example query chips in the ChatCard. Reduces first-use friction by showing users what kinds of questions they can ask.
- **Approach**: Display 4-6 example queries as clickable chips/buttons above the chat input. Clicking one populates the input and submits. Examples should be contextual — if tables are loaded, reference actual column names; if not, show generic examples.
- **Acceptance criteria**:
  - Example chips visible when chat is empty (no messages)
  - Chips disappear after first message is sent
  - At least 4 example queries covering different analysis types (spatial, aggregation, filtering, visualization)
  - If tables are loaded, examples reference actual table/column names
  - Clicking a chip submits the query
- **Files**: `src/components/ChatCard.tsx`, `src/lib/appStore.ts` (table schema for contextual examples)
- **Dependencies**: None

### TASK-26: Increased result limits with pagination (est: 3h, role: senior-engineer)
- **Description**: Increase GeoJSON feature limit to 5,000 and table row limit to 100. Add pagination to the ResultTable for navigating large result sets.
- **Approach**: Update constants in analysis execution. Add OFFSET/LIMIT pagination to the table result query. Frontend adds page navigation controls to ResultTable.
- **Acceptance criteria**:
  - Map renders up to 5,000 GeoJSON features (verify Deck.gl performance)
  - ResultTable shows up to 100 rows per page with next/prev controls
  - Page indicator shows "Page X of Y"
  - Truncation badge (TASK-20) still works with new limits
- **Files**: `src-tauri/crates/engine/src/analysis.rs` (update limits), `src-tauri/src/lib.rs`, `src/components/ChatCard.tsx` (pagination controls)
- **Dependencies**: TASK-20

---

## Phase 3: Insurance Vertical Features (Differentiation)

Goal: Build insurance-specific capabilities that transform Spatia from a generic spatial tool into a purpose-built insurance underwriting intelligence platform. This is the monetization differentiator.

### TASK-27: FEMA flood zone data integration (est: 4h, role: senior-engineer)
- **Description**: Enable loading and querying FEMA National Flood Hazard Layer (NFHL) data. This is the most critical risk overlay for property insurance underwriting.
- **Approach**: FEMA NFHL is available as Shapefile/GeoJSON from FEMA's Map Service Center. Build a Tauri command `load_fema_flood` that downloads or imports FEMA flood zone polygons for a given bounding box into DuckDB via `ST_Read`. Store as a persistent table (`fema_flood_zones`) that the AI can reference in spatial joins.
- **Acceptance criteria**:
  - New command or UI flow to load FEMA flood data for a geographic area
  - Flood zones rendered as semi-transparent polygon overlay on map
  - AI can answer queries like "What percentage of properties are in Zone AE?"
  - Flood zone data persists in DuckDB for reuse
  - Point-in-polygon spatial join works between property table and flood zones
- **Files**: `src-tauri/crates/engine/src/risk_data.rs` (new module), `src-tauri/src/lib.rs`, `src/components/MapView.tsx` (polygon overlay)
- **Dependencies**: TASK-22 (GeoJSON/Shapefile import infrastructure)

### TASK-28: USGS wildfire risk overlay (est: 3h, role: senior-engineer)
- **Description**: Integrate USGS Wildfire Hazard Potential (WHP) data as a risk overlay. WHP provides rasterized wildfire risk scores across the US.
- **Approach**: USGS WHP is available as GeoTIFF raster. Since DuckDB doesn't handle rasters natively, pre-process to vector polygons (risk zones) or use point-sampling. Alternative: use the USGS WHP web service for point-based risk lookups. Store results in DuckDB.
- **Acceptance criteria**:
  - Properties can be scored for wildfire risk (high/moderate/low)
  - Risk scores stored as a column in the property table or as a joined view
  - AI can answer "Which properties have high wildfire risk?"
  - Visual indication on map (color-coded risk)
- **Files**: `src-tauri/crates/engine/src/risk_data.rs`, `src-tauri/src/lib.rs`
- **Dependencies**: TASK-27 (shared risk data infrastructure)

### TASK-29: Insurance-specific AI system prompts (est: 3h, role: senior-engineer)
- **Description**: Replace or augment the generic analysis prompts with insurance-specific system prompts. The AI should understand property insurance terminology, common underwriting questions, risk assessment concepts, and available risk data tables.
- **Approach**: Create insurance-specific prompt templates in `spatia_ai` that inject: (1) insurance domain context (exposure, loss ratio, aggregation, zone classification), (2) available risk data tables (FEMA flood, wildfire), (3) common underwriting query patterns. Use prompt selection based on whether risk data tables are loaded.
- **Acceptance criteria**:
  - When risk data tables exist, AI uses insurance-specific system prompt
  - AI correctly uses insurance terminology in responses
  - AI generates spatial joins between property data and risk overlays without explicit instruction
  - Example queries work: "What's my portfolio exposure in flood Zone AE?", "Flag properties with combined flood and wildfire risk", "Show risk concentration by zip code"
- **Files**: `src-tauri/crates/ai/src/prompts.rs` (new insurance prompts), `src-tauri/crates/ai/src/client.rs` (prompt selection logic)
- **Dependencies**: TASK-27, TASK-28

### TASK-30: Guided risk assessment workflow (est: 5h, role: senior-engineer + ui-design-architect)
- **Description**: Build a step-by-step workflow for the insurance use case: Import Portfolio → Geocode → Load Risk Data → Risk Score → Review → Export Report. This replaces the generic "upload and chat" flow with a task-oriented experience for underwriters.
- **Approach**: New `RiskWorkflow` component that guides users through sequential steps with progress indicators. Each step maps to existing Tauri commands. The workflow is an alternative entry point — the generic chat interface remains available.
- **Acceptance criteria**:
  - Workflow accessible from a prominent UI entry point (toolbar button or welcome screen)
  - Step 1: Import property portfolio (CSV upload)
  - Step 2: Review geocoding results (show confidence, flag low matches)
  - Step 3: Select risk overlays to load (FEMA flood, wildfire)
  - Step 4: View risk assessment summary (property count by risk zone)
  - Step 5: Export results (CSV with risk scores, map PNG)
  - Each step has clear instructions and progress feedback
  - Users can skip steps or return to previous steps
- **Files**: `src/components/RiskWorkflow.tsx` (new), `src/App.tsx` (mount workflow), `src/lib/appStore.ts` (workflow state)
- **Dependencies**: TASK-27, TASK-28, TASK-14, TASK-16

### TASK-31: PDF risk assessment report generation (est: 4h, role: senior-engineer)
- **Description**: Generate a PDF report summarizing the risk assessment results. This is the key deliverable for underwriting workflows — a shareable document that can go into policy files.
- **Approach**: Use a Rust PDF generation library (e.g., `printpdf` or `genpdf`) to create a report containing: map screenshot (from TASK-16), risk summary table, property listing with risk scores, and methodology notes. Triggered from the Risk Workflow or via a "Generate Report" button.
- **Acceptance criteria**:
  - PDF includes: title page, map viewport capture, risk summary statistics, property table with risk scores
  - Generated via native save dialog
  - Professional appearance suitable for inclusion in underwriting files
  - Report data pulled from current analysis state (not re-queried)
- **Files**: `src-tauri/crates/engine/src/report.rs` (new module), `src-tauri/Cargo.toml` (PDF crate), `src-tauri/src/lib.rs` (report command)
- **Dependencies**: TASK-16, TASK-27, TASK-28, TASK-30

### TASK-32: Multi-layer map with user-controlled visibility (est: 4h, role: senior-engineer)
- **Description**: Allow users to toggle visibility of individual map layers (base data, flood zones, wildfire risk, analysis results). Essential for insurance workflows where multiple risk overlays must be compared.
- **Approach**: New `LayerPanel` component listing all active layers with visibility toggles and opacity sliders. Each data source (table points, flood polygons, wildfire zones, analysis overlay) is a separate controllable layer.
- **Acceptance criteria**:
  - Layer panel accessible from map controls (toggle button)
  - Each loaded data source appears as a layer entry
  - Visibility toggle (eye icon) shows/hides the layer
  - Opacity slider per layer
  - Layer ordering (drag to reorder) — stretch goal
  - Panel collapses to not obstruct map view
- **Files**: `src/components/LayerPanel.tsx` (new), `src/components/MapView.tsx` (layer management), `src/lib/appStore.ts` (layer visibility state)
- **Dependencies**: TASK-22, TASK-27

---

## Sprint Status

### MVP Sprint (COMPLETED)

- [x] TASK-P0-1 through TASK-13: All completed (see above)

### Post-MVP Sprint (ACTIVE)

**Phase 1 — Table Stakes (Pre-Launch Blockers): COMPLETE**
- [x] TASK-14: CSV export of any table
- [x] TASK-15: GeoJSON export of analysis_result
- [x] TASK-16: Map PNG export
- [x] TASK-17: Settings UI — API key management
- [x] TASK-18: Map legend — auto-generated
- [x] TASK-19: Basemap selector
- [x] TASK-20: Truncation indicators
- [x] TASK-21: Tooltip labels on all controls

**Phase 2 — Competitive Parity:**
- [ ] TASK-22: GeoJSON/Shapefile import
- [ ] TASK-23: Column sort/filter in table preview
- [ ] TASK-24: Editable SQL panel in chat
- [ ] TASK-25: Example query suggestions
- [ ] TASK-26: Increased result limits with pagination

**Phase 3 — Insurance Vertical (Differentiation):**
- [ ] TASK-27: FEMA flood zone data integration
- [ ] TASK-28: USGS wildfire risk overlay
- [ ] TASK-29: Insurance-specific AI prompts
- [ ] TASK-30: Guided risk assessment workflow
- [ ] TASK-31: PDF risk assessment report
- [ ] TASK-32: Multi-layer map with visibility controls

**DEFERRED:**
- [ ] TASK-P0-3: WebDriver E2E test infrastructure

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
