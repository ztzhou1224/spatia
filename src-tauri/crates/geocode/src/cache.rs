use duckdb::{params, Connection};

use crate::types::{GeoResult, GeocodeResult};

/// Create the `geocode_cache` table in `conn` if it does not already exist.
pub fn ensure_cache_table(conn: &Connection) -> GeoResult<()> {
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
) -> GeoResult<(Vec<GeocodeResult>, Vec<String>)> {
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
) -> GeoResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use duckdb::Connection;

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
}
