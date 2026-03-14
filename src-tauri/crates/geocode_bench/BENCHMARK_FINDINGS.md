# Geocoding Benchmark Findings & Improvement Plan

**Date**: 2026-03-14
**Author**: GIS Tech Lead (code analysis + data review)
**Corpus**: 500 ground truth addresses (Seattle Overture extract), 1296 variations across 7 types

---

## 1. Benchmark Infrastructure Overview

The benchmark consists of two modes:

- **TOML corpus tests** (`--skip-api`): 18 structured tests covering cache hits, local fuzzy matching, batch behavior, and edge cases. 15 tests run when skipping API-requiring tests.
- **Fuzzy accuracy benchmark** (`--fuzzy`): 1296 variations of 500 real Overture addresses, tested against a temporary DuckDB + Tantivy index. Measures match rate, correctness (within 50m haversine), confidence, and latency.

### Variation Type Distribution

| Type | Count | Description |
|------|-------|-------------|
| abbreviation | 331 | St->Street, Ave->Avenue, directionals |
| informal | 217 | Casual/shortened (e.g., "1814 18th Ave S") |
| dropped_zip | 216 | ZIP code removed |
| typo | 208 | Single-character misspellings |
| reordered | 138 | Components shuffled (e.g., ZIP before street) |
| dropped_city | 105 | City name removed |
| mixed | 81 | Combination of multiple variation types |

---

## 2. Code Analysis: Identified Weaknesses

### 2.1 Tantivy Search Strategy (search_index.rs)

**Issue A: Fuzzy matching only for tokens > 4 chars**

```rust
// Line 164-168 in search_index.rs
if !is_numeric && token.len() > 4 {
    let fuzzy_query = FuzzyTermQuery::new(term, 1, true);
    ...
}
```

This means common street name components like "pike", "main", "oak", "elm", "fir" (all 3-4 chars) get NO fuzzy matching. Typos in short street names are completely missed by Tantivy. The LIKE fallback would need to catch these, but it only fires when Tantivy returns no results at all.

**Predicted impact**: The `typo` variation type will have significantly lower accuracy for short street names. Typos like "Sreet" (5 chars, gets fuzzy) work, but "Pke" for "Pike" or "Oaak" for "Oak" would fail.

**Issue B: Fixed edit distance of 1**

FuzzyTermQuery uses distance=1, which is appropriate for most single-character typos but won't catch double-letter typos like "Pllace" (2 edits from "place"), "Avenne" (2 edits from "avenue"), or "Southwesst" (2 edits from "southwest"). Looking at the actual typo data, many variations have exactly this pattern.

**Issue C: No phrase/proximity boosting**

The BooleanQuery uses all `Occur::Should` clauses with no proximity or phrase awareness. For addresses like "123 Main Street" where the tokens are common, there's no boost for tokens appearing in sequence. This means "123 Main Street Springfield" could score similarly to a candidate containing the same tokens in a different order.

**Issue D: Score normalization floor of 0.3**

```rust
// Line 201 in search_index.rs
let normalized = if max_score > 0.0 {
    (*raw_score as f64 / max_score as f64).max(0.3)
} else {
    0.3
};
```

The 0.3 floor means the worst Tantivy result is always above `MIN_SCORE` (0.45) relative-to-best. Combined with the fact that the Tantivy path takes only the top hit (line 179: `let top = &hits[0]`), if the top hit's raw BM25 score is above 0.45, it's accepted without considering that there might be a better match via LIKE fallback.

### 2.2 LIKE Fallback (geocode.rs)

**Issue E: OR-based token filters are too permissive**

```rust
// Line 98-102 in geocode.rs
for token in tokens.iter().take(8) {
    let escaped = token.replace('\'', "''");
    token_filters.push(format!("l.label_norm LIKE '%{escaped}%'"));
}
// WHERE clause joins with OR
```

Any row matching ANY single token gets returned as a candidate. For numeric tokens like "98101" or "south", this can return hundreds of candidates, hitting the LIMIT 60 and potentially excluding the correct match.

**Issue F: tokenize_address uses raw tokens, not expanded**

The LIKE fallback tokenizes with `tokenize_address` which does NOT expand abbreviations. So if the user types "St", the LIKE filter searches for `%st%` which matches "street", "east", "state", etc. -- too many false positives. But if they type "Ave", it won't find labels containing "avenue" unless "ave" appears as a substring (which it does in "avenue", so this specific case works by accident).

**Issue G: LIMIT 60 can truncate correct matches**

With 500 addresses in the lookup table and OR-based token filters, the 60-row limit can easily exclude the correct match when common tokens (directionals, street types) produce many candidates.

### 2.3 Scoring Algorithm (scoring.rs)

**Issue H: Leading sequence bonus penalizes reordered addresses**

The `leading_ratio` (weight 0.25) compares tokens from the start of the query against the start of the label. Reordered queries like "98106 US 7774 HIGHLAND PARK Way Southwest" will score 0.0 on the leading sequence because the first tokens don't match. This is 25% of the total score lost.

**Predicted impact**: The `reordered` variation type will have the worst accuracy among all types.

**Issue I: Street number bonus is too small (5%)**

The street number bonus is only 0.05 -- the weakest signal. In practice, the street number is the most discriminating part of an address. Two addresses on the same street differ only by number. A mismatch should be penalized more heavily.

**Issue J: No string similarity for partial matches**

The scoring uses exact token matching only. No Jaro-Winkler, Levenshtein, or n-gram similarity. A typo like "Avenne" vs "Avenue" counts as a complete token mismatch in the LIKE-fallback scoring path, costing the full token's weight.

**Issue K: Asymmetric overlap calculation**

```rust
let token_overlap = overlap_count / q_tokens.len() as f64;
```

Overlap is computed as fraction of QUERY tokens found in label. This means a short query like "1814 18th Ave S" (4 tokens after normalization) matching against "1814 18th avenue south 98144" (5 tokens) gets 4/4 = 1.0 overlap, which is correct. But a long query with extra tokens not in the label gets penalized, while a short query that happens to match a few tokens of a long label gets inflated scores.

### 2.4 Text Normalization (text.rs)

**Issue L: Missing abbreviation expansions**

The abbreviation table lacks common entries used in the benchmark data:
- `"wy"` -> `"way"` (appears in data: "HIGHLAND PARK Wy SW")
- `"trl"` -> `"trail"`
- `"pt"` -> `"point"`
- `"mt"` -> `"mount"`
- `"fwy"` -> `"freeway"`
- `"expy"` -> `"expressway"`
- `"cres"` -> `"crescent"`
- `"pky"` -> `"parkway"` (alternative to "pkwy")

**Issue M: No handling of ordinal suffixes**

Tokens like "57TH", "18TH", "112TH" are kept as-is. The normalize function correctly lowercases them to "57th", "18th", etc. However, when comparing "57th" from query against "57th" in label, this works. But the Tantivy tokenizer may split "57th" differently since the default tokenizer treats it as a single token.

**Issue N: preprocess_address uses non-smart noise detection**

```rust
// text.rs line 98-99
.filter(|t| !is_noise_token(t))  // NOT is_noise_token_smart!
```

The `preprocess_address` function (used for Tantivy indexing/querying) uses `is_noise_token` which strips ALL two-letter state codes INCLUDING those that collide with street abbreviations. So "ct" (Connecticut OR Court) is always stripped, and "ne" (Nebraska OR Northeast) is always stripped. This means addresses on "Court" streets lose the street type in the Tantivy index, and "Northeast" directionals are dropped.

Wait -- the flow is: `tokenize_address` -> `expand_abbreviation` -> `is_noise_token`. Since "ct" expands to "court" and "ne" expands to "northeast", the filter checks `is_noise_token("court")` and `is_noise_token("northeast")` which return false. So this actually works correctly for expanded abbreviations. But for labels that already contain the full word "court", the expansion is a no-op (passthrough), and `is_noise_token("court")` returns false. So this path is fine.

The actual bug is more subtle: `is_noise_token` checks the EXPANDED form, but the state code list contains only abbreviated forms. So a raw state abbreviation like "wa" stays as "wa" (passthrough from `expand_abbreviation` since it's not in the abbreviation table), and then `is_noise_token("wa")` returns true. This is correct behavior.

**Correction**: The `preprocess_address` path is actually correct for most cases. However, there IS a discrepancy: `preprocess_address` applies `expand_abbreviation` then `is_noise_token`, while `normalize_for_scoring` applies `expand_abbreviation` then `is_noise_token_smart` (which preserves tokens that were expanded). Since both the Tantivy index and query go through `preprocess_address`, they're consistent with each other. The issue is that LIKE-fallback scoring and Tantivy scoring may disagree on token sets.

### 2.5 Cache Strategy (cache.rs)

**Issue O: Cache key is exact address string**

Cache lookup uses exact string match: `WHERE address = ?`. No normalization is applied. So "85 Pike St, Seattle, WA 98101" and "85 pike st, seattle, wa 98101" are different cache keys. This means the same physical address typed differently will miss cache every time.

**Issue P: Per-address cache lookups (no batch)**

```rust
// cache.rs line 29-48
for address in addresses {
    let result: duckdb::Result<GeocodeResult> = conn.query_row(
        "SELECT ... WHERE address = ?",
        params![address],
        ...
    );
}
```

Each address is a separate SQL query. For batches of 1000+ addresses, this creates 1000+ round-trips. A single `WHERE address IN (...)` or temp-table join would be significantly faster.

### 2.6 Tantivy-LIKE Handoff Logic (geocode.rs)

**Issue Q: Tantivy takes only top-1 hit**

```rust
// geocode.rs line 178-180
let top = &hits[0];
if top.score < MIN_SCORE {
    continue;
}
```

Only the top Tantivy hit is considered. If it scores above MIN_SCORE (0.45, which is very low after the 0.3 floor normalization), it's accepted. The scoring is then done by Tantivy's BM25, NOT by the custom `score_candidate` function. This means the carefully tuned weighted scoring (token overlap, leading sequence, postcode bonus) is BYPASSED for all Tantivy-resolved addresses.

The LIKE fallback uses `score_candidate`, but it only runs for addresses that Tantivy couldn't resolve at all (no index or empty results). This is a major architectural issue: the best scorer is used only as a fallback.

**Issue R: No re-scoring of Tantivy candidates**

The Tantivy `SearchHit.score` is a normalized BM25 score, not the custom weighted score. This score is used directly as the `confidence` field in results. The `score_candidate` function is never applied to Tantivy results. This means confidence values from the two paths are on different scales and not comparable.

---

## 3. Predicted Benchmark Results by Variation Type

Based on code analysis, here are predicted performance rankings (best to worst):

| Rank | Type | Predicted Correct% | Reasoning |
|------|------|-------------------|-----------|
| 1 | abbreviation | 85-95% | Tantivy preprocess expands abbreviations; good alignment |
| 2 | dropped_zip | 80-90% | Most tokens still match; postcode bonus lost but not critical |
| 3 | dropped_city | 75-85% | Most ground truth labels lack city anyway (Seattle data) |
| 4 | informal | 70-85% | Short queries match well if key tokens present |
| 5 | mixed | 60-75% | Multiple degradations compound |
| 6 | typo | 50-70% | Fuzzy only for tokens > 4 chars; edit distance 1 misses many |
| 7 | reordered | 40-60% | Leading sequence bonus (25%) completely lost; token order matters in BM25 |

---

## 4. Concrete Improvement Proposals

### Priority 1: Critical (High impact, moderate complexity)

#### TASK-GEO-01: Re-score Tantivy candidates with `score_candidate`
**What**: After Tantivy returns top-K hits, fetch their labels from DuckDB and run `score_candidate` to produce consistent confidence scores.
**Why**: Currently Tantivy's BM25 score is used as confidence, bypassing the weighted scoring algorithm. This means the postcode bonus, street number bonus, and leading sequence bonus are ignored for 95%+ of lookups.
**Where**: `geocode.rs` lines 170-215 (`tantivy_fuzzy_geocode`)
**Change**: Instead of taking `top.score` as confidence, iterate over top-5 hits, call `score_candidate(query_norm, label_norm)` for each, and pick the highest-scoring candidate.
**Expected improvement**: 5-15% increase in correct match rate, especially for reordered and mixed types.
**Complexity**: Medium (3-4h)
**Agent**: senior-engineer

#### TASK-GEO-02: Add string similarity (Jaro-Winkler) to `score_candidate`
**What**: Replace exact token matching with fuzzy token matching using Jaro-Winkler similarity for non-numeric tokens.
**Why**: Typos like "Avenne"/"Avenue", "Pllace"/"Place", "Southwesst"/"Southwest" get zero credit in current scoring. A Jaro-Winkler similarity > 0.85 should count as a partial match.
**Where**: `scoring.rs` -- modify token overlap calculation
**Change**: For each query token not found in label set, find best Jaro-Winkler match in label tokens. If similarity > 0.85, count as partial match (e.g., 0.8 weight instead of 1.0). Add `strsim` crate dependency.
**Expected improvement**: 15-25% improvement for `typo` variation type, 5-10% for `mixed`.
**Complexity**: Medium (3-4h)
**Agent**: senior-engineer

#### TASK-GEO-03: Lower Tantivy fuzzy threshold from 5 to 3 chars
**What**: Allow fuzzy matching for tokens with 3+ characters instead of 5+.
**Where**: `search_index.rs` line 165
**Change**: `if !is_numeric && token.len() > 2 {` (was `> 4`)
**Risk**: More false positives from short fuzzy matches. Mitigated by re-scoring (TASK-GEO-01).
**Expected improvement**: 5-10% improvement for `typo` variations involving short words.
**Complexity**: Low (1h)
**Agent**: senior-engineer
**Dependencies**: Should be paired with TASK-GEO-01 to avoid accepting poor fuzzy matches.

### Priority 2: High (Moderate impact, low-moderate complexity)

#### TASK-GEO-04: Add missing abbreviation expansions
**What**: Add `"wy" -> "way"`, `"trl" -> "trail"`, `"pt" -> "point"`, `"mt" -> "mount"`, `"fwy" -> "freeway"`, `"pky" -> "parkway"` to `expand_abbreviation`.
**Where**: `text.rs` lines 29-57
**Expected improvement**: Small (2-5%) but prevents hard failures on these common variations.
**Complexity**: Low (1h)
**Agent**: senior-engineer

#### TASK-GEO-05: Increase Tantivy edit distance to 2 for tokens > 6 chars
**What**: Use edit distance 2 for longer tokens (e.g., "southwest", "boulevard", "northeast") where double typos are common.
**Where**: `search_index.rs` line 166
**Change**: `let distance = if token.len() > 6 { 2 } else { 1 };`
**Expected improvement**: 5-10% improvement for typo variations on long directionals/street types.
**Complexity**: Low (1h)
**Agent**: senior-engineer

#### TASK-GEO-06: Remove leading sequence bonus or make it order-independent
**What**: Replace the rigid leading-sequence bonus with a "longest common subsequence" ratio, or reduce its weight from 0.25 to 0.10 and redistribute to token overlap.
**Why**: Leading sequence breaks entirely for reordered addresses. A 25% weight for order-dependent matching is too high for geocoding where component order varies widely.
**Where**: `scoring.rs` lines 63-68
**Proposed weights**: token_overlap 0.70, subsequence_ratio 0.15, postcode 0.10, street_num 0.05
**Expected improvement**: 15-25% improvement for `reordered` type.
**Complexity**: Medium (2-3h)
**Agent**: senior-engineer

#### TASK-GEO-07: Normalize cache keys
**What**: Normalize address strings before cache lookup/store using `normalize_address`.
**Where**: `cache.rs` -- modify `cache_lookup` and `cache_store`
**Change**: `let normalized = normalize_address(address); ... WHERE address = ?` using normalized key.
**Expected improvement**: Higher cache hit rates in production (not directly measured by benchmark, but critical for real-world performance).
**Complexity**: Low (1-2h), but requires migration for existing cache entries.
**Agent**: senior-engineer

### Priority 3: Medium (Performance optimization)

#### TASK-GEO-08: Batch cache lookups with IN clause
**What**: Replace per-address `SELECT ... WHERE address = ?` with a single batch query.
**Where**: `cache.rs` lines 29-48
**Change**: Build `WHERE address IN (?, ?, ...)` with parameter binding, or use a temp table for very large batches.
**Expected improvement**: 2-5x latency improvement for large batches (1000+ addresses).
**Complexity**: Medium (2-3h)
**Agent**: senior-engineer

#### TASK-GEO-09: Use AND+OR hybrid in LIKE fallback
**What**: Require at least 2 token matches (AND) instead of any 1 (OR) to reduce false positive candidates.
**Where**: `geocode.rs` lines 98-113
**Change**: Split tokens into "required" (street number, first 2 significant tokens) and "optional" (rest). Use `(required1 AND required2) AND (opt1 OR opt2 OR ...)`.
**Expected improvement**: Fewer false candidates, higher LIMIT utilization, faster queries.
**Complexity**: Medium (3-4h)
**Agent**: senior-engineer

#### TASK-GEO-10: Increase LIKE candidate limit from 60 to 200
**What**: Raise the LIMIT in the LIKE fallback SQL from 60 to 200.
**Where**: `geocode.rs` line 109
**Why**: With 500 addresses and OR-based filters, 60 is too restrictive. The scoring step will filter down efficiently.
**Complexity**: Trivial (0.5h)
**Agent**: senior-engineer

### Priority 4: Low (Nice-to-have)

#### TASK-GEO-11: Add Tantivy phrase query for consecutive tokens
**What**: Add a phrase query component alongside the boolean query to boost candidates where tokens appear in sequence.
**Where**: `search_index.rs` lines 142-172
**Complexity**: Medium (3-4h)
**Agent**: senior-engineer

#### TASK-GEO-12: Parallelize batch geocoding
**What**: Process addresses in parallel using rayon for the LIKE-fallback path (Tantivy is already fast).
**Where**: `geocode.rs` `local_fuzzy_geocode` function
**Complexity**: Medium (3-4h), requires careful DuckDB connection management.
**Agent**: senior-engineer

---

## 5. Recommended Implementation Order

### Sprint 1 (Critical path, ~12h total)
1. **TASK-GEO-01** (3-4h) -- Re-score Tantivy candidates. Foundational fix.
2. **TASK-GEO-02** (3-4h) -- Jaro-Winkler fuzzy token matching. Biggest accuracy win.
3. **TASK-GEO-04** (1h) -- Missing abbreviations. Quick win.
4. **TASK-GEO-03** (1h) -- Lower fuzzy char threshold. Quick win after GEO-01.

### Sprint 2 (High priority, ~8h total)
5. **TASK-GEO-06** (2-3h) -- Fix leading sequence bonus for reordered addresses.
6. **TASK-GEO-05** (1h) -- Edit distance 2 for long tokens.
7. **TASK-GEO-07** (1-2h) -- Normalize cache keys.
8. **TASK-GEO-10** (0.5h) -- Increase LIKE limit.

### Sprint 3 (Performance, ~10h total)
9. **TASK-GEO-08** (2-3h) -- Batch cache lookups.
10. **TASK-GEO-09** (3-4h) -- AND+OR hybrid LIKE filters.
11. **TASK-GEO-11** (3-4h) -- Phrase query boosting.
12. **TASK-GEO-12** (3-4h) -- Parallel batch processing.

---

## 6. Testing Strategy

After implementing Sprint 1:
- Re-run `cargo run -p spatia_geocode_bench -- --fuzzy` and compare with baseline
- Target: overall correct match rate > 85% (predicted baseline: ~70-75%)
- Target: typo variation correct rate > 75% (predicted baseline: ~55%)
- Target: reordered variation correct rate > 70% (predicted baseline: ~45%)

After implementing Sprint 2:
- Target: overall correct match rate > 90%
- Target: reordered variation correct rate > 85%
- Target: avg latency < 5ms per address

The benchmark infrastructure already supports `--compare` for regression detection between runs. Save each run's JSON output for tracking.

---

## 7. Architecture Observations

### Dual-path inconsistency is the root issue

The most impactful finding is that the Tantivy path and LIKE-fallback path use DIFFERENT scoring algorithms:
- **Tantivy path**: BM25 score (normalized, with 0.3 floor) used directly as confidence
- **LIKE-fallback path**: Custom weighted score (token overlap + leading sequence + bonuses) used as confidence

This means:
1. The custom scoring algorithm was carefully designed but is rarely exercised (only when Tantivy finds nothing)
2. Confidence values from the two paths are not comparable
3. The `MIN_LOCAL_ACCEPT_SCORE` threshold (0.75) is calibrated against one scoring function but applied to outputs from both

**TASK-GEO-01 fixes this** by applying `score_candidate` to Tantivy results. This is the single most important change.

### Cache key normalization gap

In production, users may type the same address in different formats across sessions. Without normalized cache keys, the cache is fragmented by formatting differences. A geocode cache with 10K entries might have 30% duplicate physical addresses stored under different string keys.

### Ground truth data characteristics

The Seattle Overture data has a distinctive pattern: most labels LACK a city name (just "1814 18TH Avenue South 98144 US"). This means `dropped_city` variations may perform better than expected (since the city was already missing from the label). However, variations that ADD a city name (like "SEATTLE") introduce a token that's not in the label, which could lower scores via the overlap ratio.

---

## 8. Risks & Open Questions

1. **Jaro-Winkler performance**: Adding string similarity to `score_candidate` could slow down scoring for the LIKE fallback path, which already generates up to 60 candidates per address. Need to benchmark the overhead -- likely negligible (microseconds per comparison).

2. **Abbreviation expansion conflicts**: Adding `"wy" -> "way"` could conflict with Wyoming ("WY" as state). However, state codes are already filtered by `is_noise_token_smart`, so "wy" would be expanded to "way" before the noise check. Since "way" is not a state code, it would be kept. This is correct behavior for street names but incorrect for state codes in isolation. The existing architecture handles this via context (street types expand, state codes are noise-filtered on the original).

3. **DuckDB spatial extension loading**: The `ensure_spatial_loaded` function in geocode.rs tries `LOAD` then `INSTALL + LOAD`. In benchmark mode with ephemeral DBs, this adds startup overhead. Not critical but worth noting.

4. **Test data bias**: All 500 ground truth addresses are from Seattle. Results may not generalize to other regions with different naming patterns (e.g., rural addresses, PO boxes, apartment-heavy areas).

---

## 9. Quality Gate

All changes must pass:
```bash
pnpm build
cd src-tauri && cargo test --workspace && cargo clippy --workspace
cargo run -p spatia_geocode_bench -- --fuzzy  # No regression in correct match rate
cargo run -p spatia_geocode_bench -- --skip-api  # All TOML corpus tests pass
```
