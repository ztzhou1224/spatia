use duckdb::Connection;

use crate::{EngineResult, GeocodeResult};

/// Ensure the `geocode_cache` table exists in the given DuckDB connection.
pub fn ensure_cache_table(conn: &Connection) -> EngineResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS geocode_cache (
            address   TEXT PRIMARY KEY,
            lat       REAL NOT NULL,
            lon       REAL NOT NULL,
            source    TEXT NOT NULL,
            cached_at TIMESTAMP DEFAULT current_timestamp
        );",
    )?;
    Ok(())
}

/// Look up addresses in `geocode_cache`.
///
/// Returns `(hits, misses)` where:
/// - `hits`   are fully resolved `GeocodeResult` values found in the cache.
/// - `misses` are address strings that were not present in the cache.
pub fn cache_lookup(
    conn: &Connection,
    addresses: &[String],
) -> EngineResult<(Vec<GeocodeResult>, Vec<String>)> {
    if addresses.is_empty() {
        return Ok((vec![], vec![]));
    }

    let placeholders: String = addresses
        .iter()
        .enumerate()
        .map(|(i, _)| format!("${}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        // Safe: `placeholders` contains only "$1, $2, ..." positional parameter
        // markers with no user-supplied content; values are bound via `query_map`.
        "SELECT address, lat, lon FROM geocode_cache WHERE address IN ({placeholders})"
    );

    let mut stmt = conn.prepare(&sql)?;

    let params: Vec<&dyn duckdb::ToSql> = addresses
        .iter()
        .map(|a| a as &dyn duckdb::ToSql)
        .collect();

    let rows = stmt.query_map(params.as_slice(), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, f64>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;

    let mut hits: Vec<GeocodeResult> = Vec::new();
    let mut hit_addresses: std::collections::HashSet<String> = std::collections::HashSet::new();

    for row in rows {
        let (address, lat, lon) = row?;
        hit_addresses.insert(address.clone());
        hits.push(GeocodeResult {
            address,
            lat: Some(lat),
            lon: Some(lon),
            status: None,
            error: None,
        });
    }

    let misses: Vec<String> = addresses
        .iter()
        .filter(|a| !hit_addresses.contains(*a))
        .cloned()
        .collect();

    Ok((hits, misses))
}

/// Upsert resolved geocode results into `geocode_cache`.
///
/// Only results that have both `lat` and `lon` are stored.
/// `source` should be `"sidecar"` or `"geocodio"`.
pub fn cache_store(
    conn: &Connection,
    results: &[GeocodeResult],
    source: &str,
) -> EngineResult<()> {
    let to_store: Vec<&GeocodeResult> = results
        .iter()
        .filter(|r| r.lat.is_some() && r.lon.is_some())
        .collect();

    if to_store.is_empty() {
        return Ok(());
    }

    let mut stmt = conn.prepare(
        "INSERT OR REPLACE INTO geocode_cache (address, lat, lon, source)
         VALUES ($1, $2, $3, $4)",
    )?;

    for result in to_store {
        stmt.execute(duckdb::params![
            result.address,
            result.lat.unwrap(),
            result.lon.unwrap(),
            source
        ])?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GeocodeResult;

    fn in_memory_conn() -> Connection {
        Connection::open_in_memory().expect("open in-memory duckdb")
    }

    #[test]
    fn ensure_cache_table_is_idempotent() {
        let conn = in_memory_conn();
        ensure_cache_table(&conn).expect("first call");
        ensure_cache_table(&conn).expect("second call – must be idempotent");
    }

    #[test]
    fn cache_lookup_empty_input() {
        let conn = in_memory_conn();
        ensure_cache_table(&conn).expect("setup");
        let (hits, misses) = cache_lookup(&conn, &[]).expect("lookup");
        assert!(hits.is_empty());
        assert!(misses.is_empty());
    }

    #[test]
    fn cache_store_and_lookup_round_trip() {
        let conn = in_memory_conn();
        ensure_cache_table(&conn).expect("setup");

        let results = vec![
            GeocodeResult {
                address: "123 Main St".to_string(),
                lat: Some(37.1),
                lon: Some(-122.5),
                status: None,
                error: None,
            },
            GeocodeResult {
                address: "456 Oak Ave".to_string(),
                lat: Some(38.0),
                lon: Some(-121.0),
                status: None,
                error: None,
            },
        ];

        cache_store(&conn, &results, "sidecar").expect("store");

        let addresses = vec!["123 Main St".to_string(), "789 Unknown Rd".to_string()];
        let (hits, misses) = cache_lookup(&conn, &addresses).expect("lookup");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].address, "123 Main St");
        assert!((hits[0].lat.unwrap() - 37.1).abs() < 1e-4, "lat mismatch");

        assert_eq!(misses, vec!["789 Unknown Rd".to_string()]);
    }

    #[test]
    fn cache_store_skips_unresolved_results() {
        let conn = in_memory_conn();
        ensure_cache_table(&conn).expect("setup");

        let results = vec![GeocodeResult {
            address: "No Such Place".to_string(),
            lat: None,
            lon: None,
            status: None,
            error: Some("not found".to_string()),
        }];

        cache_store(&conn, &results, "sidecar").expect("store – should not error");

        let (hits, misses) = cache_lookup(&conn, &["No Such Place".to_string()]).expect("lookup");
        assert!(hits.is_empty(), "unresolved results must not be cached");
        assert_eq!(misses.len(), 1);
    }

    #[test]
    fn cache_store_upserts_existing_entry() {
        let conn = in_memory_conn();
        ensure_cache_table(&conn).expect("setup");

        let v1 = vec![GeocodeResult {
            address: "1 Infinite Loop".to_string(),
            lat: Some(10.0),
            lon: Some(20.0),
            status: None,
            error: None,
        }];
        cache_store(&conn, &v1, "sidecar").expect("initial store");

        let v2 = vec![GeocodeResult {
            address: "1 Infinite Loop".to_string(),
            lat: Some(11.0),
            lon: Some(21.0),
            status: None,
            error: None,
        }];
        cache_store(&conn, &v2, "geocodio").expect("upsert");

        let (hits, _) = cache_lookup(&conn, &["1 Infinite Loop".to_string()]).expect("lookup");
        assert_eq!(hits.len(), 1);
        assert!((hits[0].lat.unwrap() - 11.0).abs() < 1e-4, "lat should be updated");
    }
}
