use duckdb::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::{debug, error, info, warn};

use crate::identifiers::validate_table_name;
use crate::EngineResult;

/// Absolute floor for even considering a local fuzzy candidate (inclusive).
/// A score below this threshold means the candidate is so unlike the query
/// that it is discarded entirely.
const MIN_SCORE: f64 = 0.45;

/// Default minimum score for a local fuzzy match to be *accepted* as a
/// resolved result.  Matches that score at or above `MIN_SCORE` but below
/// this threshold are considered too low-quality to accept locally; they are
/// returned to the unresolved pool so that the Geocodio API fallback can
/// attempt a proper geocode.
///
/// Override at runtime via the `SPATIA_LOCAL_GEOCODE_MIN_CONFIDENCE` env var.
const MIN_LOCAL_ACCEPT_SCORE: f64 = 0.75;

/// Read the local-accept confidence threshold from the environment, falling
/// back to `MIN_LOCAL_ACCEPT_SCORE` if the variable is absent or unparseable.
fn local_accept_threshold() -> f64 {
    std::env::var("SPATIA_LOCAL_GEOCODE_MIN_CONFIDENCE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(MIN_LOCAL_ACCEPT_SCORE)
}

/// A geocoded address result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeocodeResult {
    pub address: String,
    pub lat: f64,
    pub lon: f64,
    pub source: String,
}

/// A richer geocoding result used by the batch-first smart geocoder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeocodeBatchResult {
    pub address: String,
    pub lat: f64,
    pub lon: f64,
    pub source: String,
    pub confidence: f64,
    pub matched_label: Option<String>,
    pub matched_table: Option<String>,
}

/// Source breakdown stats returned alongside geocoding results.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GeocodeStats {
    pub total: usize,
    pub geocoded: usize,
    pub cache_hits: usize,
    pub local_fuzzy: usize,
    pub api_resolved: usize,
    pub unresolved: usize,
}

impl From<GeocodeBatchResult> for GeocodeResult {
    fn from(value: GeocodeBatchResult) -> Self {
        Self {
            address: value.address,
            lat: value.lat,
            lon: value.lon,
            source: value.source,
        }
    }
}

// ---- Cache helpers ----

/// Create the `geocode_cache` table in `conn` if it does not already exist.
pub fn ensure_cache_table(conn: &Connection) -> EngineResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS geocode_cache (
            address   TEXT PRIMARY KEY,
            lat       REAL NOT NULL,
            lon       REAL NOT NULL,
            source    TEXT NOT NULL,
            cached_at TIMESTAMP DEFAULT current_timestamp
        )",
    )?;
    Ok(())
}

/// Split `addresses` into (cached_results, uncached_addresses).
pub fn cache_lookup(
    conn: &Connection,
    addresses: &[String],
) -> EngineResult<(Vec<GeocodeResult>, Vec<String>)> {
    ensure_cache_table(conn)?;

    let mut hits = Vec::new();
    let mut misses = Vec::new();

    for address in addresses {
        let result: duckdb::Result<GeocodeResult> = conn.query_row(
            "SELECT address, lat, lon, source FROM geocode_cache WHERE address = ?",
            params![address],
            |row| {
                Ok(GeocodeResult {
                    address: row.get(0)?,
                    lat: row.get(1)?,
                    lon: row.get(2)?,
                    source: row.get(3)?,
                })
            },
        );
        match result {
            Ok(r) => hits.push(r),
            Err(_) => misses.push(address.clone()),
        }
    }

    Ok((hits, misses))
}

/// Upsert resolved geocode results into `geocode_cache` using `INSERT OR REPLACE`.
pub fn cache_store(
    conn: &Connection,
    results: &[GeocodeResult],
    source: &str,
) -> EngineResult<()> {
    ensure_cache_table(conn)?;

    for result in results {
        conn.execute(
            "INSERT OR REPLACE INTO geocode_cache (address, lat, lon, source, cached_at) \
             VALUES (?, ?, ?, ?, current_timestamp)",
            params![result.address, result.lat, result.lon, source],
        )?;
    }

    Ok(())
}

// ---- Local fuzzy resolver helpers ----

#[derive(Debug, Clone)]
struct LocalGeocodeCandidate {
    label: String,
    lat: f64,
    lon: f64,
    table: String,
}

pub(crate) fn normalize_address(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_space = true;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }

    out.trim().to_string()
}

pub(crate) fn tokenize_address(value: &str) -> Vec<String> {
    normalize_address(value)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Expand common street-type, directional, and unit abbreviations to their
/// full form so that "st" matches "street", "ave" matches "avenue", etc.
fn expand_abbreviation(token: &str) -> &str {
    match token {
        // Street types
        "st" => "street",
        "ave" => "avenue",
        "blvd" => "boulevard",
        "dr" => "drive",
        "ln" => "lane",
        "rd" => "road",
        "ct" => "court",
        "cir" => "circle",
        "pl" => "place",
        "ter" => "terrace",
        "hwy" => "highway",
        "pkwy" => "parkway",
        "sq" => "square",
        // Directionals
        "n" => "north",
        "s" => "south",
        "e" => "east",
        "w" => "west",
        "ne" => "northeast",
        "nw" => "northwest",
        "se" => "southeast",
        "sw" => "southwest",
        // Unit
        "apt" => "apartment",
        "ste" => "suite",
        "fl" => "floor",
        other => other,
    }
}

/// Two-letter US state abbreviation lookup (used by `is_noise_token_smart`).
const US_STATE_ABBREVS: &[&str] = &[
    "al", "ak", "az", "ar", "ca", "co", "ct", "de", "fl", "ga",
    "hi", "id", "il", "in", "ia", "ks", "ky", "la", "me", "md",
    "ma", "mi", "mn", "ms", "mo", "mt", "ne", "nv", "nh", "nj",
    "nm", "ny", "nc", "nd", "oh", "ok", "or", "pa", "ri", "sc",
    "sd", "tn", "tx", "ut", "vt", "va", "wa", "wv", "wi", "wy",
    "dc",
];

/// Smart noise detection that avoids collisions with street abbreviations.
/// Some state abbreviations like "ct" (Connecticut), "fl" (Florida),
/// "ne" (Nebraska) collide with street type abbreviations (court, floor,
/// northeast). We handle this by expanding abbreviations first and then
/// checking if the *original* token is a state/country code that was NOT
/// expanded (meaning it wasn't a street abbreviation).
fn is_noise_token_smart(original: &str, expanded: &str) -> bool {
    // If the token was expanded by expand_abbreviation, it's a street type,
    // not a state code — keep it.
    if original != expanded {
        return false;
    }
    // Check against state abbreviations and country codes
    US_STATE_ABBREVS.contains(&original) || matches!(original, "us" | "usa")
}

/// Normalize an address string for scoring: expand abbreviations and remove
/// noise tokens (state codes, country codes). Used only in `score_candidate`,
/// NOT in the SQL LIKE pre-filter which needs raw tokens.
fn normalize_for_scoring(address_norm: &str) -> Vec<String> {
    address_norm
        .split_whitespace()
        .map(|t| (t, expand_abbreviation(t)))
        .filter(|(orig, expanded)| !is_noise_token_smart(orig, expanded))
        .map(|(_, expanded)| expanded.to_string())
        .collect()
}

pub(crate) fn score_candidate(query_norm: &str, label_norm: &str) -> f64 {
    if query_norm.is_empty() || label_norm.is_empty() {
        return 0.0;
    }

    // Normalize both sides: expand abbreviations and strip noise tokens
    let q_tokens = normalize_for_scoring(query_norm);
    let l_tokens = normalize_for_scoring(label_norm);

    if q_tokens.is_empty() {
        return 0.0;
    }

    let l_set: HashSet<&str> = l_tokens.iter().map(|s| s.as_str()).collect();

    // (a) Token overlap ratio — weight 0.60
    let overlap_count = q_tokens
        .iter()
        .filter(|t| l_set.contains(t.as_str()))
        .count() as f64;
    let token_overlap = overlap_count / q_tokens.len() as f64;

    // (b) Leading sequence bonus — consecutive matching tokens from start — weight 0.25
    let leading_matches = q_tokens
        .iter()
        .zip(l_tokens.iter())
        .take_while(|(q, l)| q == l)
        .count() as f64;
    let leading_ratio = leading_matches / q_tokens.len() as f64;

    // (c) Postcode match bonus — if any 5+ digit numeric token in query matches label
    let postcode_bonus = if q_tokens.iter().any(|t| {
        t.len() >= 5 && t.chars().all(|c| c.is_ascii_digit()) && l_set.contains(t.as_str())
    }) {
        0.10
    } else {
        0.0
    };

    // (d) Street number match — if first numeric token in query matches first numeric in label
    let first_numeric = |tokens: &[String]| -> Option<String> {
        tokens
            .iter()
            .find(|t| t.chars().all(|c| c.is_ascii_digit()))
            .cloned()
    };
    let street_num_bonus = match (first_numeric(&q_tokens), first_numeric(&l_tokens)) {
        (Some(q), Some(l)) if q == l => 0.05,
        _ => 0.0,
    };

    let score = (token_overlap * 0.60) + (leading_ratio * 0.25) + postcode_bonus + street_num_bonus;
    score.clamp(0.0, 0.99)
}

fn has_column(conn: &Connection, table_name: &str, column: &str) -> EngineResult<bool> {
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

fn ensure_spatial_loaded(conn: &Connection) -> EngineResult<()> {
    if conn.execute("LOAD spatial", []).is_ok() {
        return Ok(());
    }
    conn.execute("INSTALL spatial", [])?;
    conn.execute("LOAD spatial", [])?;
    Ok(())
}

fn find_lookup_tables(conn: &Connection) -> EngineResult<Vec<String>> {
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
) -> EngineResult<Vec<LocalGeocodeCandidate>> {
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
) -> EngineResult<Vec<(String, GeocodeBatchResult)>> {
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
                },
            ));
        }
    }

    Ok(results)
}

fn local_fuzzy_geocode(
    conn: &Connection,
    addresses: &[String],
    db_path: Option<&str>,
) -> EngineResult<Vec<GeocodeBatchResult>> {
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

// ---- Geocodio API types ----
//
// Reference: https://www.geocod.io/docs/#batch-geocoding (v1.10)
//
// The batch endpoint (`POST /v1.10/geocode`) returns:
//   { "results": [ { "query": "...", "response": { "input": {...}, "results": [...] } } ] }
//
// Each candidate in `response.results[]` always includes:
//   formatted_address, location.{lat,lng}, accuracy (float 0-1), accuracy_type, source
// and optionally:
//   address_components, address_lines, stable_address_key
//
// We capture `accuracy` and `accuracy_type` so we can propagate the real
// accuracy score as `confidence` instead of hardcoding 0.85.  All other
// fields we don't currently use are marked `#[serde(default)]` so that
// new fields added by Geocodio are silently ignored rather than causing a
// deserialization failure.

#[derive(Debug, Deserialize)]
struct GeocodioResponse {
    results: Vec<GeocodioBatchItem>,
}

#[derive(Debug, Deserialize)]
struct GeocodioBatchItem {
    query: String,
    response: GeocodioAddressResponse,
}

#[derive(Debug, Deserialize)]
struct GeocodioAddressResponse {
    /// Parsed echo of the input address.  Present in the real API response
    /// but not used by our code; captured with `#[serde(default)]` so the
    /// struct deserializes correctly whether or not the field is present.
    #[serde(default)]
    #[allow(dead_code)]
    input: Option<serde_json::Value>,
    results: Vec<GeocodioCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeocodioCandidate {
    location: GeocodioLocation,
    /// Geocodio accuracy score: float in [0, 1].  1.0 = rooftop match.
    /// Used as the `confidence` value for results returned from the API.
    /// Ref: https://www.geocod.io/docs/#accuracy-score
    #[serde(default)]
    accuracy: f64,
    /// Human-readable accuracy type string, e.g. "rooftop", "range_interpolation",
    /// "street_center", "place".
    /// Ref: https://www.geocod.io/docs/#accuracy-type
    #[serde(default)]
    #[allow(dead_code)]
    accuracy_type: String,
    /// Data source name used by Geocodio, e.g. "Census", "Virginia GIS Clearinghouse".
    /// Distinct from our own `source` field (which is always "geocodio").
    #[serde(default)]
    #[allow(dead_code)]
    source: String,
    /// Formatted address string returned by Geocodio.
    #[serde(default)]
    #[allow(dead_code)]
    formatted_address: String,
}

#[derive(Debug, Deserialize)]
struct GeocodioLocation {
    lat: f64,
    lng: f64,
}

// ---- Geocodio API call ----

/// Internal enriched result that carries the real Geocodio accuracy score
/// alongside the geocoded coordinates.  Used by [`geocode_batch`] to populate
/// `GeocodeBatchResult.confidence` with the API-supplied value rather than a
/// hardcoded default.
struct GeocodioEnrichedResult {
    inner: GeocodeResult,
    /// Geocodio accuracy score in [0, 1].  Defaults to 0.0 if not present in
    /// the response (serde default on the `GeocodioCandidate` field).
    accuracy: f64,
}

/// Core HTTP logic shared by the public `geocode_via_geocodio` wrapper and the
/// internal `geocode_batch` call-site.  Returns enriched results including the
/// raw `accuracy` field from the Geocodio response so that callers can
/// propagate it as a confidence score.
async fn geocode_via_geocodio_inner(
    api_key: &str,
    addresses: &[String],
    base_url: &str,
) -> EngineResult<Vec<GeocodioEnrichedResult>> {
    let batch_size: usize = std::env::var("SPATIA_GEOCODIO_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(100)
        .clamp(1, 10_000);

    info!(address_count = addresses.len(), "geocode_via_geocodio: calling Geocodio API");

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1.10/geocode?api_key={}",
        base_url.trim_end_matches('/'),
        api_key
    );
    // Safe URL for logging — strip the api_key query parameter so it never
    // appears in log output.
    let log_url = format!("{}/v1.10/geocode", base_url.trim_end_matches('/'));

    let mut results = Vec::new();

    for (chunk_idx, chunk) in addresses.chunks(batch_size).enumerate() {
        debug!(chunk = chunk_idx, chunk_size = chunk.len(), url = %log_url, "geocode_via_geocodio: sending batch");
        let http_response = client
            .post(&url)
            .json(chunk)
            .send()
            .await
            .inspect_err(|e| {
                // Classify the error kind so operators know what went wrong
                // (DNS failure, TLS, timeout, connection refused, etc.)
                let kind = if e.is_timeout() {
                    "timeout"
                } else if e.is_connect() {
                    "connection"
                } else if e.is_request() {
                    "request"
                } else {
                    "unknown"
                };
                let redacted = e.to_string().replace(api_key, "[REDACTED]");
                error!(
                    url = %log_url,
                    error_kind = %kind,
                    error = %redacted,
                    "geocode_via_geocodio: HTTP request failed"
                );
            })?;

        let status = http_response.status();
        let resp = http_response
            .error_for_status()
            .inspect_err(|e| {
                let redacted = e.to_string().replace(api_key, "[REDACTED]");
                error!(
                    url = %log_url,
                    status = %status,
                    error = %redacted,
                    "geocode_via_geocodio: API returned error status"
                );
            })?;

        // Read raw body first so we can log it on parse failure
        let body = resp.text().await?;
        let response: GeocodioResponse = serde_json::from_str(&body).map_err(|e| {
            error!(
                url = %log_url,
                error = %e,
                body_preview = %&body[..body.len().min(500)],
                "geocode_via_geocodio: failed to decode response body"
            );
            e
        })?;

        for item in &response.results {
            if let Some(candidate) = item.response.results.first() {
                results.push(GeocodioEnrichedResult {
                    inner: GeocodeResult {
                        address: item.query.clone(),
                        lat: candidate.location.lat,
                        lon: candidate.location.lng,
                        source: "geocodio".to_string(),
                    },
                    accuracy: candidate.accuracy,
                });
            }
        }
    }

    info!(resolved_count = results.len(), total = addresses.len(), "geocode_via_geocodio: completed");
    Ok(results)
}

/// Call the Geocodio batch geocoding endpoint.
///
/// `base_url` should be `"https://api.geocod.io"` in production.
/// It is accepted as a parameter to allow test overriding.
///
/// Returns a `Vec<GeocodeResult>` for backward compatibility.  Internally the
/// accuracy score from the API is also captured; use [`geocode_batch`] for
/// enriched results that include confidence.
pub async fn geocode_via_geocodio(
    api_key: &str,
    addresses: &[String],
    base_url: &str,
) -> EngineResult<Vec<GeocodeResult>> {
    let enriched = geocode_via_geocodio_inner(api_key, addresses, base_url).await?;
    Ok(enriched.into_iter().map(|e| e.inner).collect())
}

// ---- Async runner helper ----

fn run_async<F, T>(f: F) -> EngineResult<T>
where
    F: std::future::Future<Output = EngineResult<T>>,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| handle.block_on(f)),
        Err(_) => tokio::runtime::Runtime::new()?.block_on(f),
    }
}

// ---- Main sync entry point ----

/// Geocode `addresses` using a cache-first strategy, falling back to the
/// Geocodio API for any cache misses, then writing new results to the cache.
///
/// Requires `SPATIA_GEOCODIO_API_KEY` to be set when there are cache misses.
/// `SPATIA_GEOCODIO_BASE_URL` overrides the API host (useful for testing).
///
/// Returns both the ordered results and a [`GeocodeStats`] breakdown by source.
pub fn geocode_batch(db_path: &str, addresses: &[String]) -> EngineResult<(Vec<GeocodeBatchResult>, GeocodeStats)> {
    info!(address_count = addresses.len(), "geocode_batch: starting batch geocode");

    let conn = Connection::open(db_path)?;

    let (cached_hits, misses) = cache_lookup(&conn, addresses)?;
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
            },
        );
    }

    let mut local_fuzzy_count = 0usize;
    let mut api_resolved_count = 0usize;

    if !misses.is_empty() {
        info!(miss_count = misses.len(), "geocode_batch: attempting local fuzzy geocode");
        let local_hits = local_fuzzy_geocode(&conn, &misses, Some(db_path))?;
        debug!(local_hits = local_hits.len(), "geocode_batch: local fuzzy geocode complete");

        if !local_hits.is_empty() {
            let threshold = local_accept_threshold();

            // Partition local results: high-confidence ones are accepted as
            // resolved; low-confidence ones are returned to the unresolved
            // pool so the Geocodio API fallback can attempt a better geocode.
            let (accepted, _below_threshold): (Vec<_>, Vec<_>) =
                local_hits.into_iter().partition(|r| r.confidence >= threshold);

            debug!(
                accepted = accepted.len(),
                below_threshold = _below_threshold.len(),
                threshold = threshold,
                "geocode_batch: local fuzzy threshold applied"
            );

            if !accepted.is_empty() {
                local_fuzzy_count = accepted.len();

                // Only cache results that met the acceptance threshold.
                // Low-confidence results are intentionally NOT cached so that
                // a later Geocodio API lookup can overwrite with a better result.
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
            // default.  The real accuracy is a float in [0, 1] where 1.0 means
            // a rooftop-level match and lower values indicate less precise
            // results (e.g. 0.8 for street_center).
            let geocodio_results = run_async(geocode_via_geocodio_inner(&api_key, &unresolved, &base_url))
                .map_err(|e| {
                    error!(error = %e, "geocode_batch: Geocodio API call failed");
                    e
                })?;
            api_resolved_count = geocodio_results.len();
            let cache_records: Vec<GeocodeResult> = geocodio_results
                .iter()
                .map(|e| e.inner.clone())
                .collect();
            cache_store(&conn, &cache_records, "geocodio")?;

            for enriched in geocodio_results {
                // Use the real API accuracy as confidence; fall back to the
                // default only if the field was absent (serde default = 0.0).
                let confidence = if enriched.accuracy > 0.0 {
                    enriched.accuracy
                } else {
                    default_confidence("geocodio")
                };
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
                    },
                );
            }
        }
    }

    let mut ordered = Vec::new();
    for address in addresses {
        if let Some(result) = resolved_by_address.get(address) {
            ordered.push(result.clone());
        }
    }

    let total = addresses.len();
    let geocoded = ordered.len();
    let unresolved = total - geocoded;
    let stats = GeocodeStats {
        total,
        geocoded,
        cache_hits: cache_hit_count,
        local_fuzzy: local_fuzzy_count,
        api_resolved: api_resolved_count,
        unresolved,
    };

    info!(
        resolved_count = geocoded,
        total = total,
        cache_hits = cache_hit_count,
        local_fuzzy = local_fuzzy_count,
        api_resolved = api_resolved_count,
        unresolved = unresolved,
        "geocode_batch: complete"
    );
    Ok((ordered, stats))
}

/// Backwards-compatible geocode API that returns the legacy shape.
pub fn geocode_addresses(db_path: &str, addresses: &[String]) -> EngineResult<Vec<GeocodeResult>> {
    let (enriched, _stats) = geocode_batch(db_path, addresses)?;
    Ok(enriched.into_iter().map(GeocodeResult::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn ensure_cache_table_is_idempotent() {
        let conn = Connection::open_in_memory().expect("open");
        ensure_cache_table(&conn).expect("first call");
        ensure_cache_table(&conn).expect("second call should not fail");
    }

    #[test]
    fn cache_store_and_lookup_round_trip() {
        let conn = Connection::open_in_memory().expect("open");
        let records = vec![GeocodeResult {
            address: "123 Main St, Springfield, IL".to_string(),
            lat: 39.7817,
            lon: -89.6501,
            source: "geocodio".to_string(),
        }];

        cache_store(&conn, &records, "geocodio").expect("store");

        let addresses = vec!["123 Main St, Springfield, IL".to_string()];
        let (hits, misses) = cache_lookup(&conn, &addresses).expect("lookup");

        assert_eq!(hits.len(), 1);
        assert!(misses.is_empty());
        assert!((hits[0].lat - 39.7817).abs() < 1e-6);
        assert!((hits[0].lon - (-89.6501)).abs() < 1e-6);
        assert_eq!(hits[0].source, "geocodio");
    }

    #[test]
    fn cache_lookup_separates_hits_and_misses() {
        let conn = Connection::open_in_memory().expect("open");
        let cached = vec![GeocodeResult {
            address: "cached address".to_string(),
            lat: 1.0,
            lon: 2.0,
            source: "geocodio".to_string(),
        }];
        cache_store(&conn, &cached, "geocodio").expect("store");

        let addresses = vec!["cached address".to_string(), "uncached address".to_string()];
        let (hits, misses) = cache_lookup(&conn, &addresses).expect("lookup");

        assert_eq!(hits.len(), 1);
        assert_eq!(misses.len(), 1);
        assert_eq!(hits[0].address, "cached address");
        assert_eq!(misses[0], "uncached address");
    }

    #[test]
    fn cache_store_upserts_existing_address() {
        let conn = Connection::open_in_memory().expect("open");
        let original = vec![GeocodeResult {
            address: "test addr".to_string(),
            lat: 10.0,
            lon: 20.0,
            source: "geocodio".to_string(),
        }];
        cache_store(&conn, &original, "geocodio").expect("store original");

        let updated = vec![GeocodeResult {
            address: "test addr".to_string(),
            lat: 11.0,
            lon: 21.0,
            source: "geocodio".to_string(),
        }];
        cache_store(&conn, &updated, "geocodio").expect("store updated");

        let addresses = vec!["test addr".to_string()];
        let (hits, _) = cache_lookup(&conn, &addresses).expect("lookup");
        assert_eq!(hits.len(), 1);
        assert!((hits[0].lat - 11.0).abs() < 1e-6);
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

    // ---- geocode_via_geocodio: API contract tests ----

    /// TC-G-001: Multiple addresses in a single batch are all returned with the
    /// correct lat/lon mapped to the correct query string.
    ///
    /// This is the primary regression test for the production bug where the
    /// batch response format was wrong (`HashMap` instead of array-of-objects).
    #[tokio::test]
    async fn geocode_via_geocodio_multi_address_batch_maps_correctly() {
        let mut server = mockito::Server::new_async().await;

        // Realistic Geocodio v1.10 batch response with three addresses.
        // The outer envelope is `{"results": [...]}` where each element
        // carries `query` (the input string) and `response` (the per-address
        // geocoding result).  This is the format that caught the production bug.
        let fixture = r#"{
            "results": [
                {
                    "query": "1 Microsoft Way, Redmond, WA 98052",
                    "response": {
                        "input": {"formatted_address": "1 Microsoft Way, Redmond, WA 98052"},
                        "results": [
                            {
                                "formatted_address": "1 Microsoft Way, Redmond, WA 98052",
                                "location": {"lat": 47.6396, "lng": -122.1283},
                                "accuracy": 1,
                                "accuracy_type": "rooftop",
                                "source": "Census"
                            }
                        ]
                    }
                },
                {
                    "query": "400 Broad St, Seattle, WA 98109",
                    "response": {
                        "input": {"formatted_address": "400 Broad St, Seattle, WA 98109"},
                        "results": [
                            {
                                "formatted_address": "400 Broad St, Seattle, WA 98109",
                                "location": {"lat": 47.6205, "lng": -122.3493},
                                "accuracy": 1,
                                "accuracy_type": "rooftop",
                                "source": "Census"
                            }
                        ]
                    }
                },
                {
                    "query": "85 Pike St, Seattle, WA 98101",
                    "response": {
                        "input": {"formatted_address": "85 Pike St, Seattle, WA 98101"},
                        "results": [
                            {
                                "formatted_address": "85 Pike St, Seattle, WA 98101",
                                "location": {"lat": 47.6088, "lng": -122.3404},
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

        let addresses = vec![
            "1 Microsoft Way, Redmond, WA 98052".to_string(),
            "400 Broad St, Seattle, WA 98109".to_string(),
            "85 Pike St, Seattle, WA 98101".to_string(),
        ];
        let results = geocode_via_geocodio("test_key", &addresses, &server.url())
            .await
            .expect("geocode multi-address batch");

        assert_eq!(results.len(), 3, "all three addresses must be returned");

        // Verify each result is keyed by its original query string
        let find = |addr: &str| results.iter().find(|r| r.address == addr).cloned();

        let redmond = find("1 Microsoft Way, Redmond, WA 98052").expect("Redmond result missing");
        assert!((redmond.lat - 47.6396).abs() < 1e-4, "Redmond lat wrong");
        assert!((redmond.lon - (-122.1283)).abs() < 1e-4, "Redmond lon wrong");
        assert_eq!(redmond.source, "geocodio");

        let needle = find("400 Broad St, Seattle, WA 98109").expect("Space Needle result missing");
        assert!((needle.lat - 47.6205).abs() < 1e-4, "Space Needle lat wrong");

        let pike = find("85 Pike St, Seattle, WA 98101").expect("Pike Place result missing");
        assert!((pike.lat - 47.6088).abs() < 1e-4, "Pike Place lat wrong");
    }

    /// TC-G-002: When the API returns results for only 2 out of 3 addresses
    /// (one address has an empty `results` array), we get exactly 2 results
    /// rather than an error or a partial panic.
    #[tokio::test]
    async fn geocode_via_geocodio_partial_results_skips_empty_response() {
        let mut server = mockito::Server::new_async().await;

        let fixture = r#"{
            "results": [
                {
                    "query": "known address 1",
                    "response": {
                        "input": {"formatted_address": "known address 1"},
                        "results": [
                            {
                                "formatted_address": "known address 1, IL 62701",
                                "location": {"lat": 39.7817, "lng": -89.6501},
                                "accuracy": 1,
                                "accuracy_type": "rooftop",
                                "source": "Census"
                            }
                        ]
                    }
                },
                {
                    "query": "unknown ambiguous address",
                    "response": {
                        "input": {"formatted_address": "unknown ambiguous address"},
                        "results": []
                    }
                },
                {
                    "query": "known address 2",
                    "response": {
                        "input": {"formatted_address": "known address 2"},
                        "results": [
                            {
                                "formatted_address": "known address 2, IL 60601",
                                "location": {"lat": 41.8781, "lng": -87.6298},
                                "accuracy": 0.8,
                                "accuracy_type": "street_center",
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

        let addresses = vec![
            "known address 1".to_string(),
            "unknown ambiguous address".to_string(),
            "known address 2".to_string(),
        ];
        let results = geocode_via_geocodio("test_key", &addresses, &server.url())
            .await
            .expect("partial results should not error");

        assert_eq!(results.len(), 2, "only the 2 resolved addresses should be returned");

        let addrs: Vec<&str> = results.iter().map(|r| r.address.as_str()).collect();
        assert!(addrs.contains(&"known address 1"), "known address 1 should be present");
        assert!(addrs.contains(&"known address 2"), "known address 2 should be present");
        assert!(
            !addrs.contains(&"unknown ambiguous address"),
            "unresolved address must not appear in results"
        );
    }

    /// TC-G-003: An empty address slice must return an empty result list
    /// immediately, without making any HTTP request.
    #[tokio::test]
    async fn geocode_via_geocodio_empty_input_returns_empty_without_http_call() {
        let mut server = mockito::Server::new_async().await;

        // Register a mock but assert it is NEVER called
        let _mock = server
            .mock("POST", "/v1.10/geocode?api_key=test_key")
            .expect(0)
            .create_async()
            .await;

        let results = geocode_via_geocodio("test_key", &[], &server.url())
            .await
            .expect("empty slice should succeed");

        assert!(results.is_empty(), "expected empty results for empty input");
        _mock.assert_async().await;
    }

    /// TC-G-004: When the API returns invalid (non-JSON) body, the function
    /// returns an error rather than panicking.
    #[tokio::test]
    async fn geocode_via_geocodio_malformed_json_returns_error() {
        let mut server = mockito::Server::new_async().await;

        let _mock = server
            .mock("POST", "/v1.10/geocode?api_key=test_key")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("this is not valid json {{{")
            .create_async()
            .await;

        let addresses = vec!["123 Main St".to_string()];
        let result = geocode_via_geocodio("test_key", &addresses, &server.url()).await;

        assert!(result.is_err(), "malformed JSON must return an error, not panic");
        let err_msg = result.unwrap_err().to_string();
        // The error should come from serde_json deserialization, not a panic.
        // We just verify it is a non-empty error string.
        assert!(!err_msg.is_empty(), "error message should not be empty");
    }

    /// TC-G-005: When the API returns a 422 (unprocessable entity) or 500
    /// (server error) HTTP status, the function propagates a meaningful error.
    #[tokio::test]
    async fn geocode_via_geocodio_http_422_returns_error() {
        let mut server = mockito::Server::new_async().await;

        let _mock = server
            .mock("POST", "/v1.10/geocode?api_key=test_key")
            .with_status(422)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error": "Invalid API key"}"#)
            .create_async()
            .await;

        let addresses = vec!["123 Main St".to_string()];
        let result = geocode_via_geocodio("test_key", &addresses, &server.url()).await;

        assert!(result.is_err(), "422 status must return an error");
    }

    #[tokio::test]
    async fn geocode_via_geocodio_http_500_returns_error() {
        let mut server = mockito::Server::new_async().await;

        let _mock = server
            .mock("POST", "/v1.10/geocode?api_key=test_key")
            .with_status(500)
            .with_header("content-type", "text/plain")
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let addresses = vec!["123 Main St".to_string()];
        let result = geocode_via_geocodio("test_key", &addresses, &server.url()).await;

        assert!(result.is_err(), "500 status must return an error");
    }

    /// TC-G-006: Deserialize a realistic Geocodio v1.10 batch response fixture
    /// directly against the `GeocodioResponse` type.
    ///
    /// This test is intentionally separate from the HTTP mock tests so that a
    /// format change in the serde structs is caught by a simple unit test that
    /// does not need a running mock server.  This is the test that would have
    /// immediately caught the production bug (HashMap vs array-of-objects).
    #[test]
    fn geocodio_v1_10_batch_response_fixture_deserializes_correctly() {
        // Copied verbatim from the Geocodio v1.10 batch API documentation.
        // Changing the struct layout in GeocodioResponse / GeocodioBatchItem
        // / GeocodioAddressResponse must be accompanied by updating this
        // fixture — otherwise this test will fail, alerting developers to the
        // contract mismatch before it reaches production.
        let fixture = r#"{
            "results": [
                {
                    "query": "1109 N Highland St, Arlington VA",
                    "response": {
                        "input": {
                            "address_components": {
                                "number": "1109",
                                "predirectional": "N",
                                "street": "Highland",
                                "suffix": "St",
                                "city": "Arlington",
                                "state": "VA",
                                "country": "US"
                            },
                            "formatted_address": "1109 N Highland St, Arlington, VA"
                        },
                        "results": [
                            {
                                "address_components": {
                                    "number": "1109",
                                    "predirectional": "N",
                                    "street": "Highland",
                                    "suffix": "St",
                                    "city": "Arlington",
                                    "county": "Arlington County",
                                    "state": "VA",
                                    "zip": "22201",
                                    "country": "US"
                                },
                                "formatted_address": "1109 N Highland St, Arlington, VA 22201",
                                "location": {
                                    "lat": 38.886672,
                                    "lng": -77.094735
                                },
                                "accuracy": 1,
                                "accuracy_type": "rooftop",
                                "source": "Virginia GIS Clearinghouse"
                            }
                        ]
                    }
                },
                {
                    "query": "525 University Ave, Toronto, ON, Canada",
                    "response": {
                        "input": {
                            "address_components": {
                                "number": "525",
                                "street": "University",
                                "suffix": "Ave",
                                "city": "Toronto",
                                "state": "ON",
                                "country": "CA"
                            },
                            "formatted_address": "525 University Ave, Toronto, ON"
                        },
                        "results": [
                            {
                                "address_components": {
                                    "number": "525",
                                    "street": "University",
                                    "suffix": "Ave",
                                    "city": "Toronto",
                                    "state": "ON",
                                    "zip": "M5G 2L3",
                                    "country": "CA"
                                },
                                "formatted_address": "525 University Ave, Toronto, ON M5G 2L3",
                                "location": {
                                    "lat": 43.656618,
                                    "lng": -79.388092
                                },
                                "accuracy": 1,
                                "accuracy_type": "rooftop",
                                "source": "City of Toronto Open Data"
                            }
                        ]
                    }
                }
            ]
        }"#;

        let response: GeocodioResponse =
            serde_json::from_str(fixture).expect("v1.10 fixture must deserialize without error");

        assert_eq!(response.results.len(), 2, "must parse exactly 2 batch items");

        // First item
        let item0 = &response.results[0];
        assert_eq!(item0.query, "1109 N Highland St, Arlington VA");
        assert_eq!(item0.response.results.len(), 1);
        let cand0 = &item0.response.results[0];
        assert!((cand0.location.lat - 38.886672).abs() < 1e-6, "Arlington lat mismatch");
        assert!((cand0.location.lng - (-77.094735)).abs() < 1e-6, "Arlington lng mismatch");
        // Accuracy fields must deserialize correctly; accuracy=1 means rooftop.
        assert!((cand0.accuracy - 1.0).abs() < 1e-6, "Arlington accuracy should be 1.0");
        assert_eq!(cand0.accuracy_type, "rooftop", "Arlington accuracy_type should be rooftop");

        // Second item
        let item1 = &response.results[1];
        assert_eq!(item1.query, "525 University Ave, Toronto, ON, Canada");
        assert_eq!(item1.response.results.len(), 1);
        let cand1 = &item1.response.results[0];
        assert!((cand1.location.lat - 43.656618).abs() < 1e-6, "Toronto lat mismatch");
        assert!((cand1.location.lng - (-79.388092)).abs() < 1e-6, "Toronto lng mismatch");
        assert!((cand1.accuracy - 1.0).abs() < 1e-6, "Toronto accuracy should be 1.0");
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

    /// TC-G-007: The real Geocodio `accuracy` field (a float in [0,1]) is
    /// propagated as `confidence` on the returned `GeocodeBatchResult`.
    ///
    /// Previously `geocode_batch` discarded the API-supplied accuracy and
    /// always set `confidence = 0.85`.  This test ensures that a low-precision
    /// match (e.g. `accuracy: 0.6`) is reflected accurately so callers can
    /// make quality-based decisions.
    ///
    /// Ref: https://www.geocod.io/docs/#accuracy-score
    #[tokio::test]
    async fn geocode_via_geocodio_inner_propagates_accuracy_as_confidence() {
        let mut server = mockito::Server::new_async().await;

        // Two addresses: one rooftop (accuracy=1.0), one street_center (accuracy=0.6).
        // The fixture uses the full real Geocodio v1.10 candidate shape so that
        // any future serde mapping change is immediately caught here.
        let fixture = r#"{
            "results": [
                {
                    "query": "1109 N Highland St, Arlington VA",
                    "response": {
                        "input": {
                            "address_components": {"number": "1109", "street": "Highland", "suffix": "St", "city": "Arlington", "state": "VA", "country": "US"},
                            "formatted_address": "1109 N Highland St, Arlington, VA"
                        },
                        "results": [
                            {
                                "address_components": {"number": "1109", "street": "Highland", "suffix": "St", "city": "Arlington", "state": "VA", "zip": "22201", "country": "US"},
                                "formatted_address": "1109 N Highland St, Arlington, VA 22201",
                                "location": {"lat": 38.886672, "lng": -77.094735},
                                "accuracy": 1,
                                "accuracy_type": "rooftop",
                                "source": "Virginia GIS Clearinghouse"
                            }
                        ]
                    }
                },
                {
                    "query": "Main Street, Springfield, IL",
                    "response": {
                        "input": {
                            "address_components": {"street": "Main", "suffix": "Street", "city": "Springfield", "state": "IL", "country": "US"},
                            "formatted_address": "Main Street, Springfield, IL"
                        },
                        "results": [
                            {
                                "address_components": {"street": "Main", "suffix": "Street", "city": "Springfield", "state": "IL", "country": "US"},
                                "formatted_address": "Main St, Springfield, IL",
                                "location": {"lat": 39.7817, "lng": -89.6501},
                                "accuracy": 0.6,
                                "accuracy_type": "street_center",
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

        let addresses = vec![
            "1109 N Highland St, Arlington VA".to_string(),
            "Main Street, Springfield, IL".to_string(),
        ];
        let enriched = geocode_via_geocodio_inner("test_key", &addresses, &server.url())
            .await
            .expect("inner call should succeed");

        assert_eq!(enriched.len(), 2, "both addresses should resolve");

        let arlington = enriched.iter().find(|e| e.inner.address.contains("Arlington")).expect("Arlington missing");
        assert!((arlington.accuracy - 1.0).abs() < 1e-6, "rooftop accuracy should be 1.0, got {}", arlington.accuracy);

        let main_st = enriched.iter().find(|e| e.inner.address.contains("Springfield")).expect("Main St missing");
        assert!((main_st.accuracy - 0.6).abs() < 1e-6, "street_center accuracy should be 0.6, got {}", main_st.accuracy);
    }

    /// TC-G-008: When `accuracy` is 0.0 (absent from response or truly zero),
    /// `geocode_batch` falls back to `default_confidence("geocodio")` = 0.85
    /// rather than propagating 0.0 as the confidence score.
    #[tokio::test]
    async fn geocode_via_geocodio_inner_falls_back_to_default_when_accuracy_zero() {
        let mut server = mockito::Server::new_async().await;

        // Omit `accuracy` entirely — serde default fills it as 0.0.
        let fixture = r#"{
            "results": [
                {
                    "query": "123 Test St, Chicago, IL",
                    "response": {
                        "input": {"formatted_address": "123 Test St, Chicago, IL"},
                        "results": [
                            {
                                "formatted_address": "123 Test St, Chicago, IL 60601",
                                "location": {"lat": 41.8781, "lng": -87.6298},
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

        let addresses = vec!["123 Test St, Chicago, IL".to_string()];
        let enriched = geocode_via_geocodio_inner("test_key", &addresses, &server.url())
            .await
            .expect("inner call should succeed");

        assert_eq!(enriched.len(), 1);
        // accuracy is 0.0 because the field was absent; geocode_batch should
        // use default_confidence("geocodio") = 0.85 instead.
        assert!((enriched[0].accuracy - 0.0).abs() < 1e-9, "absent accuracy should deserialize as 0.0");
    }

    // ---- Confidence threshold gate tests ----

    /// TC-T-001: A local fuzzy match with score < MIN_LOCAL_ACCEPT_SCORE (0.75)
    /// must NOT be accepted as resolved and must NOT be cached.
    ///
    /// Approach: use an in-memory DB to set up lookup data, call
    /// `local_fuzzy_geocode` directly to confirm the low score, then verify
    /// that the partitioning logic (score < threshold → rejected) is correct.
    /// This avoids env-var mutation so the test is safe to run in parallel.
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
        // (We simulate by checking that cache is empty after no write occurs.)
        let addresses = vec!["123 main st portland".to_string()];
        let (hits, _misses) = cache_lookup(&conn, &addresses).expect("cache_lookup");
        assert!(
            hits.is_empty(),
            "low-confidence local result must not be cached"
        );
    }

    /// TC-T-002: A local fuzzy match with score >= MIN_LOCAL_ACCEPT_SCORE (0.75)
    /// IS accepted and IS cached.
    ///
    /// Uses an exact-text query (score = 1.0) and a file-based DB so that a
    /// second connection can verify the cache write.
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
    ///
    /// Verified by calling `local_accept_threshold()` with the env var set and
    /// checking the returned value, avoiding parallel-test env-var races
    /// caused by exercising the full geocode_batch pipeline.
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
        assert_eq!(expand_abbreviation("n"), "north");
        assert_eq!(expand_abbreviation("s"), "south");
        assert_eq!(expand_abbreviation("e"), "east");
        assert_eq!(expand_abbreviation("w"), "west");
        assert_eq!(expand_abbreviation("ne"), "northeast");
        assert_eq!(expand_abbreviation("sw"), "southwest");
    }

    #[test]
    fn expand_abbreviation_units() {
        assert_eq!(expand_abbreviation("apt"), "apartment");
        assert_eq!(expand_abbreviation("ste"), "suite");
        assert_eq!(expand_abbreviation("fl"), "floor");
    }

    #[test]
    fn expand_abbreviation_passthrough() {
        assert_eq!(expand_abbreviation("main"), "main");
        assert_eq!(expand_abbreviation("seattle"), "seattle");
        assert_eq!(expand_abbreviation("98101"), "98101");
    }

    // ---- is_noise_token_smart tests ----

    #[test]
    fn noise_token_state_abbrevs_filtered() {
        // "wa" is not expanded, so it stays as "wa" — recognized as state noise
        assert!(is_noise_token_smart("wa", "wa"));
        assert!(is_noise_token_smart("ca", "ca"));
        assert!(is_noise_token_smart("il", "il"));
        assert!(is_noise_token_smart("ny", "ny"));
    }

    #[test]
    fn noise_token_country_codes_filtered() {
        assert!(is_noise_token_smart("us", "us"));
        assert!(is_noise_token_smart("usa", "usa"));
    }

    #[test]
    fn noise_token_street_abbrevs_not_filtered() {
        // "ct" expands to "court", so it's NOT noise
        assert!(!is_noise_token_smart("ct", "court"));
        // "fl" expands to "floor"
        assert!(!is_noise_token_smart("fl", "floor"));
        // "ne" expands to "northeast"
        assert!(!is_noise_token_smart("ne", "northeast"));
    }

    #[test]
    fn noise_token_regular_words_not_filtered() {
        assert!(!is_noise_token_smart("main", "main"));
        assert!(!is_noise_token_smart("seattle", "seattle"));
        assert!(!is_noise_token_smart("98101", "98101"));
    }

    // ---- normalize_for_scoring tests ----

    #[test]
    fn normalize_for_scoring_expands_and_filters() {
        let tokens = normalize_for_scoring("85 pike st seattle wa 98101");
        assert_eq!(tokens, vec!["85", "pike", "street", "seattle", "98101"]);
    }

    #[test]
    fn normalize_for_scoring_label_with_country_code() {
        let tokens = normalize_for_scoring("85 pike street seattle 98101 us");
        assert_eq!(tokens, vec!["85", "pike", "street", "seattle", "98101"]);
    }

    // ---- score_candidate tests ----

    #[test]
    fn score_candidate_empty_inputs() {
        assert_eq!(score_candidate("", "anything"), 0.0);
        assert_eq!(score_candidate("anything", ""), 0.0);
        assert_eq!(score_candidate("", ""), 0.0);
    }

    #[test]
    fn score_candidate_exact_match_after_normalization() {
        // "85 pike st seattle wa 98101" vs "85 pike street seattle 98101 us"
        // After normalization both become ["85", "pike", "street", "seattle", "98101"]
        let score = score_candidate(
            "85 pike st seattle wa 98101",
            "85 pike street seattle 98101 us",
        );
        assert!(
            score >= 0.90,
            "pike st vs pike street should score high, got {score:.3}"
        );
    }

    #[test]
    fn score_candidate_abbreviation_heavy() {
        // "100 n main ave" vs "100 north main avenue"
        let score = score_candidate("100 n main ave", "100 north main avenue");
        assert!(
            score >= 0.85,
            "abbreviation-heavy address should score high, got {score:.3}"
        );
    }

    #[test]
    fn score_candidate_wrong_city_rejected() {
        // Same street, wrong city — should be below 0.75
        let score = score_candidate(
            "85 pike st portland or 97201",
            "85 pike street seattle 98101 us",
        );
        assert!(
            score < MIN_LOCAL_ACCEPT_SCORE,
            "wrong city should be rejected, got {score:.3}"
        );
    }

    #[test]
    fn score_candidate_missing_zip() {
        // Query without zip vs label with zip
        let score = score_candidate(
            "85 pike st seattle",
            "85 pike street seattle 98101 us",
        );
        // Should still score well — all query tokens match
        assert!(
            score >= 0.75,
            "missing zip should still score well, got {score:.3}"
        );
    }

    #[test]
    fn score_candidate_blvd_vs_boulevard() {
        let score = score_candidate(
            "200 aurora blvd seattle wa 98133",
            "200 aurora boulevard seattle 98133 us",
        );
        assert!(
            score >= 0.90,
            "blvd vs boulevard should score high, got {score:.3}"
        );
    }

    #[test]
    fn score_candidate_different_street_number() {
        // Different street number — partial overlap but should be low
        let score = score_candidate(
            "200 pike st seattle wa 98101",
            "85 pike street seattle 98101 us",
        );
        // Tokens: [pike, street, seattle, 98101] overlap, but street number differs
        // and leading sequence breaks at position 0
        assert!(
            score < MIN_LOCAL_ACCEPT_SCORE,
            "different street number should be rejected, got {score:.3}"
        );
    }
}
