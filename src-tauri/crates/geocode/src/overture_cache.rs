use std::collections::HashSet;

use duckdb::Connection;
use tracing::{debug, info};

use crate::types::{GeoResult, GeocodeBatchResult};

/// The Overture release to query from S3.
const OVERTURE_RELEASE: &str = "2026-02-18.0";

/// Base S3 path for Overture addresses.
fn addresses_source() -> String {
    let release =
        std::env::var("SPATIA_OVERTURE_RELEASE").unwrap_or_else(|_| OVERTURE_RELEASE.to_string());
    format!(
        "s3://overturemaps-us-west-2/release/{}/theme=addresses/type=address/*",
        release
    )
}

/// Ensure DuckDB extensions needed for remote parquet + spatial ops are loaded.
fn ensure_extensions(conn: &Connection) -> GeoResult<()> {
    // httpfs is required for reading from S3
    conn.execute("INSTALL httpfs", []).ok();
    conn.execute("LOAD httpfs", [])?;
    // spatial is needed for ST_Y / ST_X
    conn.execute("INSTALL spatial", []).ok();
    conn.execute("LOAD spatial", [])?;
    Ok(())
}

/// Create the overture address cache table if it doesn't exist.
pub fn ensure_cache_table(conn: &Connection) -> GeoResult<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS overture_addr_cache (
            gers_id    VARCHAR PRIMARY KEY,
            number     VARCHAR,
            street     VARCHAR,
            postcode   VARCHAR,
            city       VARCHAR,
            state      VARCHAR,
            lat        DOUBLE,
            lon        DOUBLE,
            label_norm VARCHAR
        )",
        [],
    )?;
    // DuckDB doesn't support IF NOT EXISTS on indexes, so ignore errors.
    conn.execute(
        "CREATE INDEX idx_oac_postcode ON overture_addr_cache(postcode)",
        [],
    )
    .ok();
    conn.execute(
        "CREATE INDEX idx_oac_number_street ON overture_addr_cache(number, street)",
        [],
    )
    .ok();
    Ok(())
}

/// Return postcodes already present in the cache.
pub fn cached_postcodes(conn: &Connection) -> GeoResult<HashSet<String>> {
    ensure_cache_table(conn)?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT postcode FROM overture_addr_cache WHERE postcode IS NOT NULL",
    )?;
    let mut rows = stmt.query([])?;
    let mut out = HashSet::new();
    while let Some(row) = rows.next()? {
        out.insert(row.get::<_, String>(0)?);
    }
    Ok(out)
}

/// Return (city, state) pairs already present in the cache.
pub fn cached_cities(conn: &Connection) -> GeoResult<HashSet<(String, String)>> {
    ensure_cache_table(conn)?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT city, state FROM overture_addr_cache
         WHERE city IS NOT NULL AND state IS NOT NULL",
    )?;
    let mut rows = stmt.query([])?;
    let mut out = HashSet::new();
    while let Some(row) = rows.next()? {
        let city: String = row.get(0)?;
        let state: String = row.get(1)?;
        out.insert((city, state));
    }
    Ok(out)
}

/// Return state codes already fully downloaded.
/// We track this via a metadata table so we know a state-level download was done.
pub fn cached_states(conn: &Connection) -> GeoResult<HashSet<String>> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS overture_download_log (
            level  VARCHAR NOT NULL,
            key    VARCHAR NOT NULL,
            rows   BIGINT,
            PRIMARY KEY (level, key)
        )",
        [],
    )?;
    let mut stmt =
        conn.prepare("SELECT key FROM overture_download_log WHERE level = 'state'")?;
    let mut rows = stmt.query([])?;
    let mut out = HashSet::new();
    while let Some(row) = rows.next()? {
        out.insert(row.get::<_, String>(0)?);
    }
    Ok(out)
}

fn log_download(conn: &Connection, level: &str, key: &str, rows: i64) -> GeoResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO overture_download_log (level, key, rows) VALUES (?, ?, ?)",
        duckdb::params![level, key, rows],
    )?;
    Ok(())
}

/// Download Overture addresses for the given postcodes into the cache.
/// Returns the number of new rows inserted.
pub fn fetch_by_postcodes(
    conn: &Connection,
    postcodes: &[String],
    progress: impl Fn(&str, usize, usize),
) -> GeoResult<usize> {
    if postcodes.is_empty() {
        return Ok(0);
    }
    ensure_extensions(conn)?;
    ensure_cache_table(conn)?;

    let in_list = postcodes
        .iter()
        .map(|p| format!("'{}'", p.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(", ");

    progress(
        &format!("Downloading Overture addresses for {} zip codes...", postcodes.len()),
        0,
        postcodes.len(),
    );

    let sql = format!(
        "INSERT OR IGNORE INTO overture_addr_cache
         SELECT
             CAST(id AS VARCHAR) AS gers_id,
             CAST(number AS VARCHAR) AS number,
             CAST(street AS VARCHAR) AS street,
             CAST(postcode AS VARCHAR) AS postcode,
             CAST(address_levels[2].value AS VARCHAR) AS city,
             CAST(address_levels[1].value AS VARCHAR) AS state,
             CAST(ST_Y(geometry) AS DOUBLE) AS lat,
             CAST(ST_X(geometry) AS DOUBLE) AS lon,
             lower(trim(regexp_replace(
                 concat_ws(' ',
                     coalesce(CAST(number AS VARCHAR), ''),
                     coalesce(CAST(street AS VARCHAR), ''),
                     coalesce(CAST(address_levels[2].value AS VARCHAR), ''),
                     coalesce(CAST(postcode AS VARCHAR), '')
                 ),
                 '\\s+', ' '
             ))) AS label_norm
         FROM read_parquet('{source}')
         WHERE country = 'US'
           AND postcode IN ({in_list})",
        source = addresses_source(),
        in_list = in_list,
    );

    info!(postcodes = postcodes.len(), "overture_cache: fetching addresses by postcode");
    let count = conn.execute(&sql, [])?;
    info!(rows = count, "overture_cache: postcode fetch complete");

    for pc in postcodes {
        log_download(conn, "postcode", pc, count as i64).ok();
    }

    progress(
        &format!("Downloaded {} Overture address records", count),
        postcodes.len(),
        postcodes.len(),
    );

    Ok(count)
}

/// Download Overture addresses for the given (city, state) pairs.
/// Issues a single S3 query with an IN list of "CITY:STATE" composite keys.
pub fn fetch_by_cities(
    conn: &Connection,
    cities: &[(String, String)],
    progress: impl Fn(&str, usize, usize),
) -> GeoResult<usize> {
    if cities.is_empty() {
        return Ok(0);
    }
    ensure_extensions(conn)?;
    ensure_cache_table(conn)?;

    // Build composite "CITY:STATE" IN list for a single batched query.
    let in_list = cities
        .iter()
        .map(|(city, state)| {
            let safe = format!(
                "{}:{}",
                city.replace('\'', "''").to_uppercase(),
                state.replace('\'', "''").to_uppercase()
            );
            format!("'{}'", safe)
        })
        .collect::<Vec<_>>()
        .join(", ");

    progress(
        &format!("Downloading Overture addresses for {} cities...", cities.len()),
        0,
        cities.len(),
    );

    let sql = format!(
        "INSERT OR IGNORE INTO overture_addr_cache
         SELECT
             CAST(id AS VARCHAR) AS gers_id,
             CAST(number AS VARCHAR),
             CAST(street AS VARCHAR),
             CAST(postcode AS VARCHAR),
             CAST(address_levels[2].value AS VARCHAR),
             CAST(address_levels[1].value AS VARCHAR),
             CAST(ST_Y(geometry) AS DOUBLE),
             CAST(ST_X(geometry) AS DOUBLE),
             lower(trim(regexp_replace(
                 concat_ws(' ',
                     coalesce(CAST(number AS VARCHAR), ''),
                     coalesce(CAST(street AS VARCHAR), ''),
                     coalesce(CAST(address_levels[2].value AS VARCHAR), ''),
                     coalesce(CAST(postcode AS VARCHAR), '')
                 ),
                 '\\s+', ' '
             )))
         FROM read_parquet('{source}')
         WHERE country = 'US'
           AND (upper(CAST(address_levels[2].value AS VARCHAR)) || ':' || upper(CAST(address_levels[1].value AS VARCHAR))) IN ({in_list})",
        source = addresses_source(),
        in_list = in_list,
    );

    info!(cities = cities.len(), "overture_cache: fetching addresses by city batch");
    let count = conn.execute(&sql, [])?;
    info!(rows = count, "overture_cache: city batch fetch complete");

    // Log each city pair individually in the download log.
    for (city, state) in cities {
        log_download(conn, "city", &format!("{}:{}", city, state), count as i64).ok();
    }

    progress(
        &format!("Downloaded {} address records for {} cities", count, cities.len()),
        cities.len(),
        cities.len(),
    );

    Ok(count)
}

/// Download Overture addresses for entire states.
/// Issues a single S3 query with an IN list of state codes.
pub fn fetch_by_states(
    conn: &Connection,
    states: &[String],
    progress: impl Fn(&str, usize, usize),
) -> GeoResult<usize> {
    if states.is_empty() {
        return Ok(0);
    }
    ensure_extensions(conn)?;
    ensure_cache_table(conn)?;

    let in_list = states
        .iter()
        .map(|s| format!("'{}'", s.replace('\'', "''").to_uppercase()))
        .collect::<Vec<_>>()
        .join(", ");

    progress(
        &format!(
            "Downloading all Overture addresses for {} state(s)... (this may take a few minutes)",
            states.len()
        ),
        0,
        states.len(),
    );

    let sql = format!(
        "INSERT OR IGNORE INTO overture_addr_cache
         SELECT
             CAST(id AS VARCHAR) AS gers_id,
             CAST(number AS VARCHAR),
             CAST(street AS VARCHAR),
             CAST(postcode AS VARCHAR),
             CAST(address_levels[2].value AS VARCHAR),
             CAST(address_levels[1].value AS VARCHAR),
             CAST(ST_Y(geometry) AS DOUBLE),
             CAST(ST_X(geometry) AS DOUBLE),
             lower(trim(regexp_replace(
                 concat_ws(' ',
                     coalesce(CAST(number AS VARCHAR), ''),
                     coalesce(CAST(street AS VARCHAR), ''),
                     coalesce(CAST(address_levels[2].value AS VARCHAR), ''),
                     coalesce(CAST(postcode AS VARCHAR), '')
                 ),
                 '\\s+', ' '
             )))
         FROM read_parquet('{source}')
         WHERE country = 'US'
           AND upper(CAST(address_levels[1].value AS VARCHAR)) IN ({in_list})",
        source = addresses_source(),
        in_list = in_list,
    );

    info!(states = states.len(), "overture_cache: fetching addresses by state batch");
    let count = conn.execute(&sql, [])?;
    info!(rows = count, "overture_cache: state batch fetch complete");

    // Log each state individually in the download log.
    for state in states {
        log_download(conn, "state", state, count as i64).ok();
    }

    progress(
        &format!("Downloaded {} address records for {} states", count, states.len()),
        states.len(),
        states.len(),
    );

    Ok(count)
}

/// Attempt an exact match against the Overture address cache.
/// Returns a result with GERS ID if found.
pub fn exact_overture_match(
    conn: &Connection,
    number: Option<&str>,
    street: Option<&str>,
    postcode: Option<&str>,
) -> GeoResult<Option<GeocodeBatchResult>> {
    let (Some(number), Some(street), Some(postcode)) = (number, street, postcode) else {
        return Ok(None);
    };

    if number.is_empty() || street.is_empty() || postcode.is_empty() {
        return Ok(None);
    }

    let safe_number = number.replace('\'', "''");
    let safe_street = crate::text::normalize_address(street);
    let safe_postcode = postcode.replace('\'', "''");

    // Try exact match on number + normalized street tokens + postcode
    let sql = format!(
        "SELECT gers_id, number, street, city, state, postcode, lat, lon, label_norm
         FROM overture_addr_cache
         WHERE number = '{number}'
           AND postcode = '{postcode}'
           AND label_norm LIKE '%{street}%'
         LIMIT 5",
        number = safe_number,
        postcode = safe_postcode,
        street = safe_street,
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    // Score each candidate and pick the best
    let mut best: Option<(GeocodeBatchResult, f64)> = None;

    while let Some(row) = rows.next()? {
        let gers_id: String = row.get(0)?;
        let label_norm: String = row.get::<_, String>(8).unwrap_or_default();
        let query_norm = crate::text::normalize_address(&format!("{} {} {}", number, street, postcode));
        let score = crate::scoring::score_candidate(&query_norm, &label_norm);

        if score < 0.90 {
            continue; // Only accept high-confidence exact matches
        }

        let candidate = GeocodeBatchResult {
            address: String::new(), // will be filled by caller
            lat: row.get::<_, f64>(6).unwrap_or(0.0),
            lon: row.get::<_, f64>(7).unwrap_or(0.0),
            source: "overture_exact".to_string(),
            confidence: score.min(0.98),
            matched_label: Some(label_norm),
            matched_table: Some("overture_addr_cache".to_string()),
            gers_id: Some(gers_id),
        };

        match &best {
            Some((_, best_score)) if score <= *best_score => {}
            _ => best = Some((candidate, score)),
        }
    }

    Ok(best.map(|(r, _)| r))
}

/// Fuzzy match against the Overture address cache for a given address.
/// Uses LIKE-based candidate retrieval + scoring.
pub fn fuzzy_overture_match(
    conn: &Connection,
    address: &str,
    postcode: Option<&str>,
    _city: Option<&str>,
    state: Option<&str>,
) -> GeoResult<Option<GeocodeBatchResult>> {
    let query_norm = crate::text::normalize_address(address);
    if query_norm.is_empty() {
        return Ok(None);
    }

    let tokens = crate::text::tokenize_address(address);
    if tokens.is_empty() {
        return Ok(None);
    }

    // Build WHERE clause with available filters
    let mut filters = Vec::new();
    if let Some(pc) = postcode {
        if !pc.is_empty() {
            filters.push(format!("postcode = '{}'", pc.replace('\'', "''")));
        }
    }
    if let Some(st) = state {
        if !st.is_empty() {
            filters.push(format!("upper(state) = '{}'", st.replace('\'', "''").to_uppercase()));
        }
    }

    // Add token LIKE filters (up to 6 tokens)
    let mut token_filters = Vec::new();
    for token in tokens.iter().take(6) {
        let escaped = token.replace('\'', "''");
        token_filters.push(format!("label_norm LIKE '%{escaped}%'"));
    }

    let where_clause = if filters.is_empty() {
        token_filters.join(" OR ")
    } else {
        format!("({}) AND ({})", filters.join(" AND "), token_filters.join(" OR "))
    };

    let sql = format!(
        "SELECT gers_id, label_norm, lat, lon
         FROM overture_addr_cache
         WHERE {where_clause}
         LIMIT 60"
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut best: Option<(GeocodeBatchResult, f64)> = None;

    while let Some(row) = rows.next()? {
        let gers_id: String = row.get(0)?;
        let label_norm: String = row.get::<_, String>(1).unwrap_or_default();
        let score = crate::scoring::score_candidate(&query_norm, &label_norm);

        if score < crate::scoring::MIN_SCORE {
            continue;
        }

        let candidate = GeocodeBatchResult {
            address: String::new(),
            lat: row.get::<_, f64>(2).unwrap_or(0.0),
            lon: row.get::<_, f64>(3).unwrap_or(0.0),
            source: "overture_fuzzy".to_string(),
            confidence: score,
            matched_label: Some(label_norm),
            matched_table: Some("overture_addr_cache".to_string()),
            gers_id: Some(gers_id),
        };

        match &best {
            Some((_, best_score)) if score <= *best_score => {}
            _ => best = Some((candidate, score)),
        }
    }

    Ok(best.map(|(r, _)| r))
}

/// Batch reverse lookup: given a list of (lat, lon, postcode) tuples, find the
/// nearest Overture address record for each to attach GERS IDs.
/// Returns a Vec of Option<String> in the same order as the input.
pub fn batch_reverse_lookup_gers(
    conn: &Connection,
    coords: &[(f64, f64, Option<&str>)],
) -> GeoResult<Vec<Option<String>>> {
    if coords.is_empty() {
        return Ok(Vec::new());
    }

    // For each coordinate, run a small query. This is still per-item but
    // we prepare the statement once and reuse it for the common case (no postcode filter).
    let mut results = Vec::with_capacity(coords.len());

    // Most addresses won't have a postcode filter, so prepare two statements
    let sql_no_pc = "SELECT gers_id FROM overture_addr_cache \
                     WHERE lat IS NOT NULL AND lon IS NOT NULL \
                     ORDER BY pow(lat - ?, 2) + pow(lon - ?, 2) \
                     LIMIT 1";
    let mut stmt_no_pc = conn.prepare(sql_no_pc)?;

    for (lat, lon, postcode) in coords {
        let gers_id = if let Some(pc) = postcode {
            if !pc.is_empty() {
                // Use postcode-filtered query for better accuracy
                let sql_pc = format!(
                    "SELECT gers_id FROM overture_addr_cache \
                     WHERE lat IS NOT NULL AND lon IS NOT NULL \
                       AND postcode = '{}' \
                     ORDER BY pow(lat - {}, 2) + pow(lon - {}, 2) \
                     LIMIT 1",
                    pc.replace('\'', "''"),
                    lat,
                    lon,
                );
                let mut stmt = conn.prepare(&sql_pc)?;
                let mut rows = stmt.query([])?;
                if let Some(row) = rows.next()? {
                    Some(row.get::<_, String>(0)?)
                } else {
                    None
                }
            } else {
                let mut rows = stmt_no_pc.query(duckdb::params![lat, lon])?;
                if let Some(row) = rows.next()? {
                    Some(row.get::<_, String>(0)?)
                } else {
                    None
                }
            }
        } else {
            let mut rows = stmt_no_pc.query(duckdb::params![lat, lon])?;
            if let Some(row) = rows.next()? {
                Some(row.get::<_, String>(0)?)
            } else {
                None
            }
        };
        results.push(gers_id);
    }

    Ok(results)
}

/// Reverse lookup: given coordinates (from Geocodio), find the nearest Overture
/// address record to attach a GERS ID.
pub fn reverse_lookup_gers(
    conn: &Connection,
    lat: f64,
    lon: f64,
    postcode: Option<&str>,
) -> GeoResult<Option<String>> {
    let pc_filter = match postcode {
        Some(pc) if !pc.is_empty() => {
            format!("AND postcode = '{}'", pc.replace('\'', "''"))
        }
        _ => String::new(),
    };

    let sql = format!(
        "SELECT gers_id
         FROM overture_addr_cache
         WHERE lat IS NOT NULL AND lon IS NOT NULL
           {pc_filter}
         ORDER BY pow(lat - {lat}, 2) + pow(lon - {lon}, 2)
         LIMIT 1",
        pc_filter = pc_filter,
        lat = lat,
        lon = lon,
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    if let Some(row) = rows.next()? {
        Ok(Some(row.get::<_, String>(0)?))
    } else {
        debug!(lat, lon, "reverse_lookup_gers: no nearby Overture record found");
        Ok(None)
    }
}
