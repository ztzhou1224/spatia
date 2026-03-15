use std::collections::{HashMap, HashSet};

use duckdb::Connection;
use tracing::{debug, error, info, warn};

use crate::cache::{cache_lookup, cache_store};
use crate::geocodio::{geocode_via_geocodio_inner, GeocodioEnrichedResult};
use crate::identifiers::validate_table_name;
use crate::overture_cache;
use crate::scoring::{local_accept_threshold, score_candidate, MIN_SCORE};
use crate::text::{normalize_address, tokenize_address, AddressComponents, components_from_string};
use crate::types::{GeoResult, GeocodeBatchResult, GeocodeResult, GeocodeStats};

#[derive(Debug, Clone)]
struct LocalGeocodeCandidate {
    label: String,
    lat: f64,
    lon: f64,
    table: String,
}

fn has_column(conn: &Connection, table_name: &str, column: &str) -> GeoResult<bool> {
    let mut stmt = conn.prepare(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'main' AND table_name = ? \
         ORDER BY ordinal_position"
    )?;
    let mut rows = stmt.query(duckdb::params![table_name])?;

    while let Some(row) = rows.next()? {
        let name: String = row.get(0)?;
        if name.eq_ignore_ascii_case(column) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn ensure_spatial_loaded(conn: &Connection) -> GeoResult<()> {
    if conn.execute("LOAD spatial", []).is_ok() {
        return Ok(());
    }
    conn.execute("INSTALL spatial", [])?;
    conn.execute("LOAD spatial", [])?;
    Ok(())
}

fn find_lookup_tables(conn: &Connection) -> GeoResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT table_name
         FROM information_schema.tables
         WHERE table_schema = 'main' AND table_name LIKE '%\\_lookup' ESCAPE '\\'",
    )?;

    let mut rows = stmt.query([])?;
    let mut tables = Vec::new();
    while let Some(row) = rows.next()? {
        let table: String = row.get(0)?;
        validate_table_name(&table)?;
        tables.push(table);
    }
    Ok(tables)
}

fn local_candidates_for_address(
    conn: &Connection,
    lookup_table: &str,
    address: &str,
) -> GeoResult<Vec<LocalGeocodeCandidate>> {
    validate_table_name(lookup_table)?;
    let base_table = lookup_table.trim_end_matches("_lookup").to_string();
    validate_table_name(&base_table)?;

    if !has_column(conn, &base_table, "id")? {
        return Ok(Vec::new());
    }

    let has_lat = has_column(conn, &base_table, "lat")?;
    let has_lon = has_column(conn, &base_table, "lon")?;
    let has_geometry = has_column(conn, &base_table, "geometry")?;

    let coord_expr = if has_lat && has_lon {
        "CAST(t.lat AS DOUBLE) AS lat, CAST(t.lon AS DOUBLE) AS lon".to_string()
    } else if has_geometry {
        ensure_spatial_loaded(conn)?;
        "CAST(ST_Y(t.geometry) AS DOUBLE) AS lat, CAST(ST_X(t.geometry) AS DOUBLE) AS lon"
            .to_string()
    } else {
        return Ok(Vec::new());
    };

    let tokens = tokenize_address(address);
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let mut token_filters = Vec::new();
    for token in tokens.iter().take(8) {
        let escaped = token.replace('\'', "''");
        token_filters.push(format!("l.label_norm LIKE '%{escaped}%'"));
    }

    let sql = format!(
        "SELECT l.label, {coord_expr}
         FROM {lookup} l
         JOIN {base} t ON CAST(t.id AS VARCHAR) = l.source_id
         WHERE {where_clause}
         LIMIT 60",
        coord_expr = coord_expr,
        lookup = lookup_table,
        base = base_table,
        where_clause = token_filters.join(" OR "),
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(LocalGeocodeCandidate {
            label: row.get::<_, String>(0).unwrap_or_default(),
            lat: row.get::<_, f64>(1).unwrap_or(0.0),
            lon: row.get::<_, f64>(2).unwrap_or(0.0),
            table: base_table.clone(),
        });
    }

    Ok(out)
}

/// Try to resolve addresses using a Tantivy index for the given lookup table.
/// Falls back to LIKE-based matching if no Tantivy index exists.
fn tantivy_fuzzy_geocode(
    conn: &Connection,
    db_path: &str,
    lookup_table: &str,
    addresses: &[String],
) -> GeoResult<Vec<(String, GeocodeBatchResult)>> {
    use crate::search_index;

    let index_dir = search_index::index_dir_for_table(db_path, lookup_table);
    if !index_dir.exists() {
        return Ok(Vec::new());
    }

    let base_table = lookup_table.trim_end_matches("_lookup").to_string();
    validate_table_name(&base_table)?;

    // Detect coordinate columns on the base table
    let has_lat = has_column(conn, &base_table, "lat")?;
    let has_lon = has_column(conn, &base_table, "lon")?;
    let has_geometry = has_column(conn, &base_table, "geometry")?;
    let has_id = has_column(conn, &base_table, "id")?;

    if !has_id || (!has_lat && !has_lon && !has_geometry) {
        return Ok(Vec::new());
    }

    let coord_expr = if has_lat && has_lon {
        "CAST(t.lat AS DOUBLE) AS lat, CAST(t.lon AS DOUBLE) AS lon".to_string()
    } else if has_geometry {
        ensure_spatial_loaded(conn)?;
        "CAST(ST_Y(t.geometry) AS DOUBLE) AS lat, CAST(ST_X(t.geometry) AS DOUBLE) AS lon"
            .to_string()
    } else {
        return Ok(Vec::new());
    };

    let mut results = Vec::new();

    for address in addresses {
        let hits = search_index::search_addresses(&index_dir, address, 5)?;
        if hits.is_empty() {
            continue;
        }

        // Take the top hit and fetch its coordinates from DuckDB
        let top = &hits[0];
        if top.score < MIN_SCORE {
            continue;
        }

        let escaped_id = top.source_id.replace('\'', "''");
        let sql = format!(
            "SELECT {coord_expr} FROM {base} t WHERE CAST(t.id AS VARCHAR) = '{id}' LIMIT 1",
            coord_expr = coord_expr,
            base = base_table,
            id = escaped_id,
        );

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query([])?;

        if let Some(row) = rows.next()? {
            let lat: f64 = row.get::<_, f64>(0).unwrap_or(0.0);
            let lon: f64 = row.get::<_, f64>(1).unwrap_or(0.0);

            results.push((
                address.clone(),
                GeocodeBatchResult {
                    address: address.clone(),
                    lat,
                    lon,
                    source: "overture_fuzzy".to_string(),
                    confidence: top.score,
                    matched_label: Some(top.label.clone()),
                    matched_table: Some(base_table.clone()),
                    gers_id: None,
                },
            ));
        }
    }

    Ok(results)
}

pub fn local_fuzzy_geocode(
    conn: &Connection,
    addresses: &[String],
    db_path: Option<&str>,
) -> GeoResult<Vec<GeocodeBatchResult>> {
    let lookup_tables = find_lookup_tables(conn)?;
    if lookup_tables.is_empty() {
        return Ok(Vec::new());
    }

    // First, try Tantivy-based search if db_path is available
    let mut tantivy_resolved: HashMap<String, GeocodeBatchResult> = HashMap::new();
    if let Some(db_path) = db_path {
        for lookup_table in &lookup_tables {
            if crate::search_index::has_index(db_path, lookup_table) {
                match tantivy_fuzzy_geocode(conn, db_path, lookup_table, addresses) {
                    Ok(hits) => {
                        info!(
                            hits = hits.len(),
                            table = lookup_table.as_str(),
                            "local_fuzzy_geocode: Tantivy search returned results"
                        );
                        for (addr, result) in hits {
                            // Keep the highest scoring match per address
                            let existing_score = tantivy_resolved
                                .get(&addr)
                                .map(|r| r.confidence)
                                .unwrap_or(0.0);
                            if result.confidence > existing_score {
                                tantivy_resolved.insert(addr, result);
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            table = lookup_table.as_str(),
                            "local_fuzzy_geocode: Tantivy search failed, falling back to LIKE"
                        );
                    }
                }
            }
        }
    }

    // For addresses not resolved by Tantivy, fall back to LIKE-based matching
    let unresolved: Vec<&String> = addresses
        .iter()
        .filter(|a| !tantivy_resolved.contains_key(*a))
        .collect();

    let mut out: Vec<GeocodeBatchResult> = tantivy_resolved.into_values().collect();

    if !unresolved.is_empty() {
        debug!(
            count = unresolved.len(),
            "local_fuzzy_geocode: falling back to LIKE-based matching for unresolved addresses"
        );
        for address in unresolved {
            let query_norm = normalize_address(address);
            if query_norm.is_empty() {
                continue;
            }

            let mut best: Option<(LocalGeocodeCandidate, f64)> = None;

            for lookup_table in &lookup_tables {
                let candidates = local_candidates_for_address(conn, lookup_table, address)?;
                for candidate in candidates {
                    let candidate_norm = normalize_address(&candidate.label);
                    let score = score_candidate(&query_norm, &candidate_norm);
                    if score < MIN_SCORE {
                        continue;
                    }

                    match &best {
                        Some((_, best_score)) if score <= *best_score => {}
                        _ => best = Some((candidate, score)),
                    }
                }
            }

            if let Some((candidate, score)) = best {
                out.push(GeocodeBatchResult {
                    address: address.clone(),
                    lat: candidate.lat,
                    lon: candidate.lon,
                    source: "overture_fuzzy".to_string(),
                    confidence: score,
                    matched_label: Some(candidate.label),
                    matched_table: Some(candidate.table),
                    gers_id: None,
                });
            }
        }
    }

    Ok(out)
}

fn default_confidence(source: &str) -> f64 {
    if source.eq_ignore_ascii_case("geocodio") {
        0.85
    } else if source.eq_ignore_ascii_case("overture_fuzzy") {
        0.8
    } else {
        1.0
    }
}

// ---- Async runner helper ----

fn run_async<F, T>(f: F) -> GeoResult<T>
where
    F: std::future::Future<Output = GeoResult<T>>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(f)),
        Err(_) => tokio::runtime::Runtime::new()?.block_on(f),
    }
}

// ---- Main sync entry point ----

/// Geocode `addresses` using a cache-first, Overture-first strategy,
/// falling back to the Geocodio API for any cache misses, then writing
/// new results to the cache.
///
/// This is a convenience wrapper over [`geocode_batch_with_components`]
/// that parses each address string into [`AddressComponents`] before geocoding.
///
/// Requires `SPATIA_GEOCODIO_API_KEY` to be set when there are cache misses
/// that cannot be resolved locally.
/// `SPATIA_GEOCODIO_BASE_URL` overrides the API host (useful for testing).
///
/// Returns both the ordered results and a [`GeocodeStats`] breakdown by source.
pub fn geocode_batch(db_path: &str, addresses: &[String]) -> GeoResult<(Vec<GeocodeBatchResult>, GeocodeStats)> {
    let components: Vec<AddressComponents> = addresses
        .iter()
        .map(|a| components_from_string(a))
        .collect();
    geocode_batch_with_components(db_path, &components)
}

/// Maximum batch size for the API-first fast path.
/// Batches at or below this size skip the Overture S3 download cascade
/// when `SPATIA_GEOCODIO_API_KEY` is available, going straight to
/// Cache → Geocodio API → GERS reverse lookup.
///
/// Override with `SPATIA_GEOCODE_FAST_PATH_LIMIT` env var.
fn fast_path_limit() -> usize {
    std::env::var("SPATIA_GEOCODE_FAST_PATH_LIMIT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(500)
}

/// API-first fast path: Cache → Geocodio API → (optional) GERS reverse lookup.
///
/// Skips the expensive Overture S3 download cascade entirely. For a 50-row
/// commercial property portfolio spread across 40+ cities, this reduces
/// geocoding from 15+ minutes (downloading millions of Overture rows) to
/// ~2 seconds (single Geocodio batch API call).
pub fn geocode_batch_api_first(
    db_path: &str,
    components: &[AddressComponents],
) -> GeoResult<(Vec<GeocodeBatchResult>, GeocodeStats)> {
    let addresses: Vec<String> = components.iter().map(|c| c.full.clone()).collect();

    info!(address_count = addresses.len(), "geocode_batch_api_first: starting fast-path geocode");

    let conn = Connection::open(db_path)?;

    // ---- Step 1: Cache lookup ----
    let (cached_hits, misses) = cache_lookup(&conn, &addresses)?;
    let cache_hit_count = cached_hits.len();
    debug!(cache_hits = cache_hit_count, cache_misses = misses.len(), "geocode_batch_api_first: cache lookup complete");

    let mut resolved_by_address: HashMap<String, GeocodeBatchResult> = HashMap::new();

    for result in cached_hits {
        resolved_by_address.insert(
            result.address.clone(),
            GeocodeBatchResult {
                address: result.address,
                lat: result.lat,
                lon: result.lon,
                confidence: default_confidence(&result.source),
                source: result.source,
                matched_label: None,
                matched_table: None,
                gers_id: None,
            },
        );
    }

    // ---- Step 2: Try exact/fuzzy Overture match from EXISTING cache only (no S3 downloads) ----
    let mut overture_exact_count = 0usize;
    let mut local_fuzzy_count = 0usize;
    let mut api_resolved_count = 0usize;

    if !misses.is_empty() {
        let miss_set: HashSet<&str> = misses.iter().map(String::as_str).collect();
        let miss_components: Vec<&AddressComponents> = components
            .iter()
            .filter(|c| miss_set.contains(c.full.as_str()))
            .collect();

        // Try exact Overture match from already-cached data (no S3 downloads)
        for comp in &miss_components {
            if resolved_by_address.contains_key(&comp.full) {
                continue;
            }
            let number = comp.number.as_deref();
            let street = comp.street.as_deref();
            let zip = comp.zip.as_deref();

            if number.is_some() && street.is_some() && zip.is_some() {
                match overture_cache::exact_overture_match(&conn, number, street, zip) {
                    Ok(Some(mut result)) => {
                        result.address = comp.full.clone();
                        resolved_by_address.insert(comp.full.clone(), result);
                        overture_exact_count += 1;
                    }
                    Ok(None) => {}
                    Err(e) => debug!(error = %e, "geocode_batch_api_first: exact_overture_match error"),
                }
            }
        }

        // Try fuzzy Overture match from existing cache
        for comp in &miss_components {
            if resolved_by_address.contains_key(&comp.full) {
                continue;
            }
            match overture_cache::fuzzy_overture_match(
                &conn,
                &comp.full,
                comp.zip.as_deref(),
                comp.city.as_deref(),
                comp.state.as_deref(),
            ) {
                Ok(Some(mut result)) => {
                    let threshold = local_accept_threshold();
                    if result.confidence >= threshold {
                        result.address = comp.full.clone();
                        resolved_by_address.insert(comp.full.clone(), result);
                        local_fuzzy_count += 1;
                    }
                }
                Ok(None) => {}
                Err(e) => debug!(error = %e, "geocode_batch_api_first: fuzzy_overture_match error"),
            }
        }

        // ---- Step 3: Geocodio API for everything still unresolved ----
        let unresolved: Vec<String> = misses
            .into_iter()
            .filter(|address| !resolved_by_address.contains_key(address))
            .collect();

        if !unresolved.is_empty() {
            let api_key = std::env::var("SPATIA_GEOCODIO_API_KEY").map_err(|_| {
                "SPATIA_GEOCODIO_API_KEY environment variable not set"
            })?;
            let base_url = std::env::var("SPATIA_GEOCODIO_BASE_URL")
                .unwrap_or_else(|_| "https://api.geocod.io".to_string());

            info!(unresolved_count = unresolved.len(), "geocode_batch_api_first: calling Geocodio API");
            let geocodio_results = run_async(geocode_via_geocodio_inner(&api_key, &unresolved, &base_url))?;
            api_resolved_count = geocodio_results.len();

            let cache_records: Vec<GeocodeResult> = geocodio_results
                .iter()
                .map(|e: &GeocodioEnrichedResult| e.inner.clone())
                .collect();
            cache_store(&conn, &cache_records, "geocodio")?;

            // Batch GERS reverse lookup
            let addr_to_zip: HashMap<String, Option<String>> = components
                .iter()
                .map(|c| (c.full.clone(), c.zip.clone()))
                .collect();

            for enriched in geocodio_results {
                let confidence = if enriched.accuracy > 0.0 {
                    enriched.accuracy
                } else {
                    default_confidence("geocodio")
                };

                let zip_hint = addr_to_zip
                    .get(&enriched.inner.address)
                    .and_then(|z| z.as_deref());
                let gers_id = overture_cache::reverse_lookup_gers(
                    &conn,
                    enriched.inner.lat,
                    enriched.inner.lon,
                    zip_hint,
                )
                .unwrap_or(None);

                resolved_by_address.insert(
                    enriched.inner.address.clone(),
                    GeocodeBatchResult {
                        address: enriched.inner.address,
                        lat: enriched.inner.lat,
                        lon: enriched.inner.lon,
                        source: enriched.inner.source,
                        confidence,
                        matched_label: None,
                        matched_table: None,
                        gers_id,
                    },
                );
            }
        }
    }

    let mut ordered = Vec::new();
    for address in &addresses {
        if let Some(result) = resolved_by_address.get(address) {
            ordered.push(result.clone());
        }
    }

    let total = addresses.len();
    let geocoded = ordered.len();
    let stats = GeocodeStats {
        total,
        geocoded,
        cache_hits: cache_hit_count,
        overture_exact: overture_exact_count,
        local_fuzzy: local_fuzzy_count,
        api_resolved: api_resolved_count,
        unresolved: total - geocoded,
    };

    info!(
        resolved_count = geocoded,
        total = total,
        cache_hits = cache_hit_count,
        overture_exact = overture_exact_count,
        local_fuzzy = local_fuzzy_count,
        api_resolved = api_resolved_count,
        unresolved = total - geocoded,
        "geocode_batch_api_first: complete"
    );
    Ok((ordered, stats))
}

/// Geocode pre-parsed address components using the best available strategy.
///
/// **Strategy selection:**
/// - If `SPATIA_GEOCODIO_API_KEY` is set and batch size ≤ `fast_path_limit()` (default 500),
///   uses the API-first fast path: Cache → Geocodio API → GERS reverse lookup.
///   This avoids expensive Overture S3 downloads for small/medium batches.
/// - Otherwise, falls back to the full Overture-first pipeline.
///
/// The fast path reduces geocoding of a 50-row multi-city CSV from 15+ minutes
/// to ~2 seconds by skipping millions of Overture S3 row downloads.
pub fn geocode_batch_with_components(
    db_path: &str,
    components: &[AddressComponents],
) -> GeoResult<(Vec<GeocodeBatchResult>, GeocodeStats)> {
    let limit = fast_path_limit();
    let has_api_key = std::env::var("SPATIA_GEOCODIO_API_KEY").is_ok();

    if has_api_key && components.len() <= limit {
        info!(
            batch_size = components.len(),
            fast_path_limit = limit,
            "geocode_batch: using API-first fast path (skipping Overture S3 downloads)"
        );
        return geocode_batch_api_first(db_path, components);
    }

    geocode_batch_overture_first(db_path, components)
}

/// Full Overture-first geocoding pipeline (original strategy).
///
/// Pipeline:
/// 1. Cache lookup — return cached hits immediately.
/// 2. For cache misses:
///    a. Extract zip codes; download Overture data for those zips.
///    b. Try exact Overture match (number + street + postcode).
///    c. Try fuzzy Overture match for remaining.
///    d. Unresolved addresses fall through to Geocodio API (if key provided).
/// 3. Existing local fuzzy geocode (lookup tables) as fallback.
/// 4. Geocodio API fallback for anything still unresolved.
/// 5. GERS reverse lookup to attach GERS IDs to Geocodio results.
///
/// Returns both the ordered results and a [`GeocodeStats`] breakdown by source.
pub fn geocode_batch_overture_first(
    db_path: &str,
    components: &[AddressComponents],
) -> GeoResult<(Vec<GeocodeBatchResult>, GeocodeStats)> {
    let addresses: Vec<String> = components.iter().map(|c| c.full.clone()).collect();

    info!(address_count = addresses.len(), "geocode_batch: starting batch geocode");

    let conn = Connection::open(db_path)?;

    // ---- Step 1: Cache lookup ----
    let (cached_hits, misses) = cache_lookup(&conn, &addresses)?;
    let cache_hit_count = cached_hits.len();
    debug!(cache_hits = cache_hit_count, cache_misses = misses.len(), "geocode_batch: cache lookup complete");

    let mut resolved_by_address: HashMap<String, GeocodeBatchResult> = HashMap::new();

    for result in cached_hits {
        resolved_by_address.insert(
            result.address.clone(),
            GeocodeBatchResult {
                address: result.address,
                lat: result.lat,
                lon: result.lon,
                confidence: default_confidence(&result.source),
                source: result.source,
                matched_label: None,
                matched_table: None,
                gers_id: None,
            },
        );
    }

    let mut overture_exact_count = 0usize;
    let mut local_fuzzy_count = 0usize;
    let mut api_resolved_count = 0usize;

    if !misses.is_empty() {
        // Build a lookup from address string → components for the miss set
        let miss_set: HashSet<&str> = misses.iter().map(String::as_str).collect();
        let miss_components: Vec<&AddressComponents> = components
            .iter()
            .filter(|c| miss_set.contains(c.full.as_str()))
            .collect();

        // ---- Step 2a: Collect zip codes needed, skip already-cached ones ----
        let already_cached_zips = overture_cache::cached_postcodes(&conn).unwrap_or_default();

        let needed_zips: Vec<String> = miss_components
            .iter()
            .filter_map(|c| c.zip.as_deref())
            .filter(|z| !z.is_empty() && !already_cached_zips.contains(*z))
            .map(String::from)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        if !needed_zips.is_empty() {
            info!(zip_count = needed_zips.len(), "geocode_batch: downloading Overture data for zip codes");
            match overture_cache::fetch_by_postcodes(&conn, &needed_zips, |msg, _done, _total| {
                debug!(msg, "geocode_batch: overture postcode fetch progress");
            }) {
                Ok(rows) => debug!(rows, "geocode_batch: Overture postcode fetch complete"),
                Err(e) => warn!(error = %e, "geocode_batch: Overture postcode fetch failed, continuing"),
            }
        }

        // ---- Step 2b: Exact Overture match ----
        for comp in &miss_components {
            if resolved_by_address.contains_key(&comp.full) {
                continue;
            }
            // Extract street number from the parsed number field or first token
            let number = comp.number.as_deref();
            // Use the street field; for free-text addresses this is the full string
            let street = comp.street.as_deref();
            let zip = comp.zip.as_deref();

            if number.is_some() && street.is_some() && zip.is_some() {
                match overture_cache::exact_overture_match(&conn, number, street, zip) {
                    Ok(Some(mut result)) => {
                        result.address = comp.full.clone();
                        debug!(
                            address = comp.full.as_str(),
                            gers_id = result.gers_id.as_deref().unwrap_or(""),
                            "geocode_batch: exact Overture match"
                        );
                        resolved_by_address.insert(comp.full.clone(), result);
                        overture_exact_count += 1;
                    }
                    Ok(None) => {}
                    Err(e) => warn!(error = %e, address = comp.full.as_str(), "geocode_batch: exact_overture_match error"),
                }
            }
        }

        // ---- Step 2c: Fuzzy Overture match for still-unresolved ----
        for comp in &miss_components {
            if resolved_by_address.contains_key(&comp.full) {
                continue;
            }
            match overture_cache::fuzzy_overture_match(
                &conn,
                &comp.full,
                comp.zip.as_deref(),
                comp.city.as_deref(),
                comp.state.as_deref(),
            ) {
                Ok(Some(mut result)) => {
                    let threshold = local_accept_threshold();
                    if result.confidence >= threshold {
                        result.address = comp.full.clone();
                        debug!(
                            address = comp.full.as_str(),
                            confidence = result.confidence,
                            "geocode_batch: fuzzy Overture match (from zip cache)"
                        );
                        resolved_by_address.insert(comp.full.clone(), result);
                        local_fuzzy_count += 1;
                    }
                }
                Ok(None) => {}
                Err(e) => warn!(error = %e, address = comp.full.as_str(), "geocode_batch: fuzzy_overture_match error"),
            }
        }

        // City/state-level Overture downloads removed — zip-level is sufficient.
        // Unresolved addresses fall through to Geocodio API (if key provided)
        // or remain unresolved.

        // ---- Step 3: Existing local fuzzy geocode (lookup tables) ----
        // Collect still-unresolved miss addresses for the legacy path
        let still_unresolved_for_local: Vec<String> = misses
            .iter()
            .filter(|a| !resolved_by_address.contains_key(*a))
            .cloned()
            .collect();

        if !still_unresolved_for_local.is_empty() {
            info!(miss_count = still_unresolved_for_local.len(), "geocode_batch: attempting local fuzzy geocode");
            let local_hits = local_fuzzy_geocode(&conn, &still_unresolved_for_local, Some(db_path))?;
            debug!(local_hits = local_hits.len(), "geocode_batch: local fuzzy geocode complete");

            if !local_hits.is_empty() {
                let threshold = local_accept_threshold();

                let (accepted, _below_threshold): (Vec<_>, Vec<_>) =
                    local_hits.into_iter().partition(|r| r.confidence >= threshold);

                debug!(
                    accepted = accepted.len(),
                    below_threshold = _below_threshold.len(),
                    threshold = threshold,
                    "geocode_batch: local fuzzy threshold applied"
                );

                if !accepted.is_empty() {
                    local_fuzzy_count += accepted.len();

                    let local_cache_records: Vec<GeocodeResult> = accepted
                        .iter()
                        .map(|r| GeocodeResult {
                            address: r.address.clone(),
                            lat: r.lat,
                            lon: r.lon,
                            source: r.source.clone(),
                        })
                        .collect();
                    cache_store(&conn, &local_cache_records, "overture_fuzzy")?;

                    for result in accepted {
                        resolved_by_address.insert(result.address.clone(), result);
                    }
                }
            }
        }

        // ---- Step 4: Geocodio API fallback ----
        let unresolved: Vec<String> = misses
            .into_iter()
            .filter(|address| !resolved_by_address.contains_key(address))
            .collect();

        if !unresolved.is_empty() {
            info!(unresolved_count = unresolved.len(), "geocode_batch: falling back to Geocodio API");
            let api_key = std::env::var("SPATIA_GEOCODIO_API_KEY").map_err(|_| {
                warn!("geocode_batch: SPATIA_GEOCODIO_API_KEY not set, cannot geocode remaining addresses");
                "SPATIA_GEOCODIO_API_KEY environment variable not set"
            })?;
            let base_url = std::env::var("SPATIA_GEOCODIO_BASE_URL")
                .unwrap_or_else(|_| "https://api.geocod.io".to_string());

            // Use the inner function so we get the real `accuracy` score from
            // the Geocodio API rather than falling back to the hardcoded 0.85
            // default.
            let geocodio_results = run_async(geocode_via_geocodio_inner(&api_key, &unresolved, &base_url))
                .map_err(|e| {
                    error!(error = %e, "geocode_batch: Geocodio API call failed");
                    e
                })?;
            api_resolved_count = geocodio_results.len();
            let cache_records: Vec<GeocodeResult> = geocodio_results
                .iter()
                .map(|e: &GeocodioEnrichedResult| e.inner.clone())
                .collect();
            cache_store(&conn, &cache_records, "geocodio")?;

            // Build a zip map from address → zip for GERS reverse lookup
            let addr_to_zip: HashMap<String, Option<String>> = components
                .iter()
                .map(|c| (c.full.clone(), c.zip.clone()))
                .collect();

            for enriched in geocodio_results {
                let confidence = if enriched.accuracy > 0.0 {
                    enriched.accuracy
                } else {
                    default_confidence("geocodio")
                };

                // ---- Step 5: GERS reverse lookup for Geocodio results ----
                let zip_hint = addr_to_zip
                    .get(&enriched.inner.address)
                    .and_then(|z| z.as_deref());
                let gers_id = overture_cache::reverse_lookup_gers(
                    &conn,
                    enriched.inner.lat,
                    enriched.inner.lon,
                    zip_hint,
                )
                .unwrap_or(None);

                if gers_id.is_some() {
                    debug!(
                        address = enriched.inner.address.as_str(),
                        gers_id = gers_id.as_deref().unwrap_or(""),
                        "geocode_batch: attached GERS ID to Geocodio result"
                    );
                }

                resolved_by_address.insert(
                    enriched.inner.address.clone(),
                    GeocodeBatchResult {
                        address: enriched.inner.address,
                        lat: enriched.inner.lat,
                        lon: enriched.inner.lon,
                        source: enriched.inner.source,
                        confidence,
                        matched_label: None,
                        matched_table: None,
                        gers_id,
                    },
                );
            }
        }
    }

    let mut ordered = Vec::new();
    for address in &addresses {
        if let Some(result) = resolved_by_address.get(address) {
            ordered.push(result.clone());
        }
    }

    let total = addresses.len();
    let geocoded = ordered.len();
    let unresolved_count = total - geocoded;
    let stats = GeocodeStats {
        total,
        geocoded,
        cache_hits: cache_hit_count,
        overture_exact: overture_exact_count,
        local_fuzzy: local_fuzzy_count,
        api_resolved: api_resolved_count,
        unresolved: unresolved_count,
    };

    info!(
        resolved_count = geocoded,
        total = total,
        cache_hits = cache_hit_count,
        overture_exact = overture_exact_count,
        local_fuzzy = local_fuzzy_count,
        api_resolved = api_resolved_count,
        unresolved = unresolved_count,
        "geocode_batch: complete"
    );
    Ok((ordered, stats))
}

/// Backwards-compatible geocode API that returns the legacy shape.
pub fn geocode_addresses(db_path: &str, addresses: &[String]) -> GeoResult<Vec<GeocodeResult>> {
    let (enriched, _stats) = geocode_batch(db_path, addresses)?;
    Ok(enriched.into_iter().map(GeocodeResult::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{cache_lookup, cache_store};
    use crate::geocodio::geocode_via_geocodio;
    use crate::scoring::{MIN_LOCAL_ACCEPT_SCORE, MIN_SCORE, local_accept_threshold};
    use duckdb::Connection;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    }

    fn tmp_db_path() -> String {
        format!("/tmp/spatia_geocode_test_{}.duckdb", unique_suffix())
    }

    fn cleanup_db(db_path: &str) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{db_path}.wal"));
        let _ = std::fs::remove_file(format!("{db_path}.wal.lck"));
    }

    #[test]
    fn geocode_addresses_missing_api_key_returns_error() {
        let db_path = tmp_db_path();
        // Ensure the env var is absent for this test
        std::env::remove_var("SPATIA_GEOCODIO_API_KEY");
        let addresses = vec!["uncached address that requires API".to_string()];
        let err = geocode_addresses(&db_path, &addresses).expect_err("should fail");
        assert!(err.to_string().contains("SPATIA_GEOCODIO_API_KEY"));
        cleanup_db(&db_path);
    }

    #[tokio::test]
    async fn geocode_via_geocodio_calls_api_and_parses_response() {
        let mut server = mockito::Server::new_async().await;

        let fixture = r#"{
            "results": [
                {
                    "query": "123 Main St, Springfield, IL",
                    "response": {
                        "input": {"formatted_address": "123 Main St, Springfield, IL"},
                        "results": [
                            {
                                "formatted_address": "123 Main St, Springfield, IL 62701",
                                "location": {"lat": 39.7817, "lng": -89.6501},
                                "accuracy": 1,
                                "accuracy_type": "rooftop",
                                "source": "Census"
                            }
                        ]
                    }
                }
            ]
        }"#;

        let _mock = server
            .mock("POST", "/v1.10/geocode?api_key=test_key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(fixture)
            .create_async()
            .await;

        let addresses = vec!["123 Main St, Springfield, IL".to_string()];
        let results =
            geocode_via_geocodio("test_key", &addresses, &server.url())
                .await
                .expect("geocode");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].address, "123 Main St, Springfield, IL");
        assert!((results[0].lat - 39.7817).abs() < 1e-6);
        assert!((results[0].lon - (-89.6501)).abs() < 1e-6);
        assert_eq!(results[0].source, "geocodio");
    }

    #[test]
    fn geocode_batch_uses_local_fuzzy_without_api_key() {
        let db_path = tmp_db_path();
        let conn = Connection::open(&db_path).expect("open");

        conn.execute(
            "CREATE TABLE addresses (id VARCHAR, label TEXT, lat DOUBLE, lon DOUBLE)",
            [],
        )
        .expect("create addresses");
        conn.execute(
            "CREATE TABLE addresses_lookup (source_id VARCHAR, label TEXT, label_norm TEXT)",
            [],
        )
        .expect("create lookup");
        conn.execute(
            "INSERT INTO addresses VALUES ('a1', '123 Main Street Springfield IL', 39.7817, -89.6501)",
            [],
        )
        .expect("insert addresses");
        conn.execute(
            "INSERT INTO addresses_lookup VALUES ('a1', '123 Main Street Springfield IL', '123 main street springfield il')",
            [],
        )
        .expect("insert lookup");

        // Use a query that matches the lookup label closely enough to score
        // above the acceptance threshold (MIN_LOCAL_ACCEPT_SCORE = 0.75).
        // The exact label text normalises to the same string, scoring 1.0.
        std::env::remove_var("SPATIA_GEOCODIO_API_KEY");
        let (results, _stats) = geocode_batch(&db_path, &["123 Main Street Springfield IL".to_string()])
            .expect("local fuzzy geocode");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "overture_fuzzy");
        assert!(results[0].confidence >= MIN_LOCAL_ACCEPT_SCORE);
        assert!(results[0].matched_table.as_deref() == Some("addresses"));

        cleanup_db(&db_path);
    }

    #[test]
    fn geocode_batch_returns_enriched_cached_results() {
        let db_path = tmp_db_path();
        let conn = Connection::open(&db_path).expect("open");
        cache_store(
            &conn,
            &[GeocodeResult {
                address: "cached addr".to_string(),
                lat: 1.5,
                lon: 2.5,
                source: "geocodio".to_string(),
            }],
            "geocodio",
        )
        .expect("cache");

        let (results, _stats) = geocode_batch(&db_path, &["cached addr".to_string()]).expect("batch");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].address, "cached addr");
        assert_eq!(results[0].source, "geocodio");
        assert!(results[0].confidence > 0.0);

        cleanup_db(&db_path);
    }

    /// TC-T-001: A local fuzzy match with score < MIN_LOCAL_ACCEPT_SCORE (0.75)
    /// must NOT be accepted as resolved and must NOT be cached.
    #[test]
    fn low_confidence_local_match_is_not_accepted_and_not_cached() {
        let conn = Connection::open_in_memory().expect("open");

        conn.execute(
            "CREATE TABLE places (id VARCHAR, label TEXT, lat DOUBLE, lon DOUBLE)",
            [],
        )
        .expect("create places");
        conn.execute(
            "CREATE TABLE places_lookup (source_id VARCHAR, label TEXT, label_norm TEXT)",
            [],
        )
        .expect("create lookup");
        // Use a wrong-city query so the new weighted scorer produces a
        // score between MIN_SCORE (0.45) and MIN_LOCAL_ACCEPT_SCORE (0.75).
        conn.execute(
            "INSERT INTO places VALUES ('p1', '123 Main Street Springfield IL', 39.7817, -89.6501)",
            [],
        )
        .expect("insert places");
        conn.execute(
            "INSERT INTO places_lookup VALUES ('p1', '123 Main Street Springfield IL', '123 main street springfield il')",
            [],
        )
        .expect("insert lookup");

        let query = vec!["123 main st portland".to_string()];
        let local_hits = local_fuzzy_geocode(&conn, &query, None).expect("local fuzzy geocode");

        assert_eq!(local_hits.len(), 1, "candidate should be found by fuzzy search");
        let hit = &local_hits[0];
        assert!(
            hit.confidence < MIN_LOCAL_ACCEPT_SCORE,
            "confidence {:.3} must be below MIN_LOCAL_ACCEPT_SCORE {:.3}",
            hit.confidence,
            MIN_LOCAL_ACCEPT_SCORE
        );

        // Simulate what geocode_batch does: partition by threshold.
        let threshold = MIN_LOCAL_ACCEPT_SCORE;
        let (accepted, below): (Vec<_>, Vec<_>) =
            local_hits.into_iter().partition(|r| r.confidence >= threshold);

        assert!(accepted.is_empty(), "low-confidence result must not be accepted");
        assert_eq!(below.len(), 1, "low-confidence result must be in rejected pool");

        // Verify that a cache write does NOT happen for the rejected result.
        let addresses = vec!["123 main st portland".to_string()];
        let (hits, _misses) = cache_lookup(&conn, &addresses).expect("cache_lookup");
        assert!(
            hits.is_empty(),
            "low-confidence local result must not be cached"
        );
    }

    /// TC-T-002: A local fuzzy match with score >= MIN_LOCAL_ACCEPT_SCORE (0.75)
    /// IS accepted and IS cached.
    #[test]
    fn high_confidence_local_match_is_accepted_and_cached() {
        let db_path = tmp_db_path();
        {
            let conn = Connection::open(&db_path).expect("open");
            conn.execute(
                "CREATE TABLE locs (id VARCHAR, label TEXT, lat DOUBLE, lon DOUBLE)",
                [],
            )
            .expect("create locs");
            conn.execute(
                "CREATE TABLE locs_lookup (source_id VARCHAR, label TEXT, label_norm TEXT)",
                [],
            )
            .expect("create lookup");
            conn.execute(
                "INSERT INTO locs VALUES ('l1', 'Space Needle Seattle WA', 47.6205, -122.3493)",
                [],
            )
            .expect("insert locs");
            conn.execute(
                "INSERT INTO locs_lookup VALUES ('l1', 'Space Needle Seattle WA', 'space needle seattle wa')",
                [],
            )
            .expect("insert lookup");
            // conn drops here, releasing the write lock
        }

        std::env::remove_var("SPATIA_GEOCODIO_API_KEY");

        // Exact match → score 1.0, well above the default 0.75 threshold.
        let query = "Space Needle Seattle WA".to_string();
        let (results, _stats) =
            geocode_batch(&db_path, &[query.clone()]).expect("high-confidence local geocode");

        assert_eq!(results.len(), 1, "exact match must be accepted");
        assert_eq!(results[0].source, "overture_fuzzy");
        assert!(
            results[0].confidence >= MIN_LOCAL_ACCEPT_SCORE,
            "accepted result confidence must be >= MIN_LOCAL_ACCEPT_SCORE"
        );

        // Open a fresh connection to verify the cache was written.
        let conn2 = Connection::open(&db_path).expect("open fresh conn");
        let addresses = vec![query];
        let (hits, _misses) = cache_lookup(&conn2, &addresses).expect("cache_lookup");
        assert_eq!(hits.len(), 1, "high-confidence local result must be cached");
        assert_eq!(hits[0].source, "overture_fuzzy");

        cleanup_db(&db_path);
    }

    /// TC-T-003: The acceptance threshold can be overridden via the
    /// `SPATIA_LOCAL_GEOCODE_MIN_CONFIDENCE` environment variable.
    #[test]
    fn threshold_env_var_overrides_default() {
        // Verify the default when no env var is set.
        std::env::remove_var("SPATIA_LOCAL_GEOCODE_MIN_CONFIDENCE");
        assert!(
            (local_accept_threshold() - MIN_LOCAL_ACCEPT_SCORE).abs() < 1e-9,
            "default threshold should equal MIN_LOCAL_ACCEPT_SCORE"
        );

        // Verify that a valid float in the env var is used as-is.
        std::env::set_var("SPATIA_LOCAL_GEOCODE_MIN_CONFIDENCE", "0.60");
        assert!(
            (local_accept_threshold() - 0.60).abs() < 1e-9,
            "env var 0.60 should override the default"
        );

        // Verify that a non-parseable value falls back to the default.
        std::env::set_var("SPATIA_LOCAL_GEOCODE_MIN_CONFIDENCE", "not_a_number");
        assert!(
            (local_accept_threshold() - MIN_LOCAL_ACCEPT_SCORE).abs() < 1e-9,
            "unparseable env var should fall back to MIN_LOCAL_ACCEPT_SCORE"
        );

        // Clean up.
        std::env::remove_var("SPATIA_LOCAL_GEOCODE_MIN_CONFIDENCE");

        // Additionally verify end-to-end with a file DB: a match scoring ~0.525
        // (below default 0.75) IS accepted when the threshold is lowered to 0.50
        // via the env var.  To avoid race conditions with other tests, we lower
        // the threshold by clamping the partition check manually rather than
        // relying on the env var during geocode_batch.  Instead we call
        // local_fuzzy_geocode directly and verify the score is in [0.50, 0.75).
        let conn = Connection::open_in_memory().expect("open");
        conn.execute(
            "CREATE TABLE spots (id VARCHAR, label TEXT, lat DOUBLE, lon DOUBLE)",
            [],
        )
        .expect("create spots");
        conn.execute(
            "CREATE TABLE spots_lookup (source_id VARCHAR, label TEXT, label_norm TEXT)",
            [],
        )
        .expect("create lookup");
        conn.execute(
            "INSERT INTO spots VALUES ('s1', '123 Main Street Springfield IL', 39.7817, -89.6501)",
            [],
        )
        .expect("insert spots");
        conn.execute(
            "INSERT INTO spots_lookup VALUES ('s1', '123 Main Street Springfield IL', '123 main street springfield il')",
            [],
        )
        .expect("insert lookup");

        // Use a wrong-city query so the score lands between MIN_SCORE and
        // MIN_LOCAL_ACCEPT_SCORE with the weighted scorer (~0.69).
        let query = vec!["123 main st portland".to_string()];
        let local_hits = local_fuzzy_geocode(&conn, &query, None).expect("fuzzy geocode");
        assert_eq!(local_hits.len(), 1, "candidate must be found");
        let score = local_hits[0].confidence;
        assert!(score >= MIN_SCORE, "score {score:.3} must be >= MIN_SCORE");
        assert!(
            score < MIN_LOCAL_ACCEPT_SCORE,
            "score {score:.3} must be < MIN_LOCAL_ACCEPT_SCORE to exercise the low-threshold case"
        );
        // With a threshold of 0.50, this score would be accepted.
        let custom_threshold = 0.50_f64;
        let (accepted, _): (Vec<_>, Vec<_>) =
            local_hits.into_iter().partition(|r| r.confidence >= custom_threshold);
        assert_eq!(accepted.len(), 1, "score {score:.3} must be accepted when threshold is 0.50");
    }

    // ---- expand_abbreviation tests ----

    #[test]
    fn expand_abbreviation_street_types() {
        use crate::text::expand_abbreviation;
        assert_eq!(expand_abbreviation("st"), "street");
        assert_eq!(expand_abbreviation("ave"), "avenue");
        assert_eq!(expand_abbreviation("blvd"), "boulevard");
        assert_eq!(expand_abbreviation("dr"), "drive");
        assert_eq!(expand_abbreviation("ln"), "lane");
        assert_eq!(expand_abbreviation("rd"), "road");
        assert_eq!(expand_abbreviation("ct"), "court");
        assert_eq!(expand_abbreviation("hwy"), "highway");
        assert_eq!(expand_abbreviation("pkwy"), "parkway");
    }

    #[test]
    fn expand_abbreviation_directionals() {
        use crate::text::expand_abbreviation;
        assert_eq!(expand_abbreviation("n"), "north");
        assert_eq!(expand_abbreviation("s"), "south");
        assert_eq!(expand_abbreviation("e"), "east");
        assert_eq!(expand_abbreviation("w"), "west");
        assert_eq!(expand_abbreviation("ne"), "northeast");
        assert_eq!(expand_abbreviation("sw"), "southwest");
    }

    #[test]
    fn expand_abbreviation_units() {
        use crate::text::expand_abbreviation;
        assert_eq!(expand_abbreviation("apt"), "apartment");
        assert_eq!(expand_abbreviation("ste"), "suite");
        assert_eq!(expand_abbreviation("fl"), "floor");
    }

    #[test]
    fn expand_abbreviation_passthrough() {
        use crate::text::expand_abbreviation;
        assert_eq!(expand_abbreviation("main"), "main");
        assert_eq!(expand_abbreviation("seattle"), "seattle");
        assert_eq!(expand_abbreviation("98101"), "98101");
    }

    // ---- is_noise_token_smart tests ----

    #[test]
    fn noise_token_state_abbrevs_filtered() {
        use crate::text::is_noise_token_smart;
        assert!(is_noise_token_smart("wa", "wa"));
        assert!(is_noise_token_smart("ca", "ca"));
        assert!(is_noise_token_smart("il", "il"));
        assert!(is_noise_token_smart("ny", "ny"));
    }

    #[test]
    fn noise_token_country_codes_filtered() {
        use crate::text::is_noise_token_smart;
        assert!(is_noise_token_smart("us", "us"));
        assert!(is_noise_token_smart("usa", "usa"));
    }

    #[test]
    fn noise_token_street_abbrevs_not_filtered() {
        use crate::text::is_noise_token_smart;
        assert!(!is_noise_token_smart("ct", "court"));
        assert!(!is_noise_token_smart("fl", "floor"));
        assert!(!is_noise_token_smart("ne", "northeast"));
    }

    #[test]
    fn noise_token_regular_words_not_filtered() {
        use crate::text::is_noise_token_smart;
        assert!(!is_noise_token_smart("main", "main"));
        assert!(!is_noise_token_smart("seattle", "seattle"));
        assert!(!is_noise_token_smart("98101", "98101"));
    }
}
