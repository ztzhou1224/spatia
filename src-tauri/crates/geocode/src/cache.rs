use duckdb::Connection;

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
///
/// Uses a single `WHERE address IN (...)` query instead of one query per
/// address, reducing DuckDB round-trips from N to 1.
pub fn cache_lookup(
    conn: &Connection,
    addresses: &[String],
) -> GeoResult<(Vec<GeocodeResult>, Vec<String>)> {
    ensure_cache_table(conn)?;

    if addresses.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    // Build a single IN-list query for all addresses at once.
    // For very large batches we chunk to avoid SQL statement size limits,
    // but for typical geocoding batches (≤10k) a single query is fine.
    const CHUNK_SIZE: usize = 500;
    let mut hit_map: std::collections::HashMap<String, GeocodeResult> =
        std::collections::HashMap::with_capacity(addresses.len());

    for chunk in addresses.chunks(CHUNK_SIZE) {
        let placeholders: String = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT address, lat, lon, source FROM geocode_cache WHERE address IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn duckdb::ToSql> =
            chunk.iter().map(|a| a as &dyn duckdb::ToSql).collect();
        let mut rows = stmt.query(params.as_slice())?;
        while let Some(row) = rows.next()? {
            let result = GeocodeResult {
                address: row.get(0)?,
                lat: row.get(1)?,
                lon: row.get(2)?,
                source: row.get(3)?,
            };
            hit_map.insert(result.address.clone(), result);
        }
    }

    let mut hits = Vec::with_capacity(hit_map.len());
    let mut misses = Vec::with_capacity(addresses.len() - hit_map.len());
    for address in addresses {
        if let Some(result) = hit_map.remove(address) {
            hits.push(result);
        } else {
            misses.push(address.clone());
        }
    }

    Ok((hits, misses))
}

/// Upsert resolved geocode results into `geocode_cache` using a single
/// multi-row `INSERT OR REPLACE` statement per chunk.
pub fn cache_store(
    conn: &Connection,
    results: &[GeocodeResult],
    source: &str,
) -> GeoResult<()> {
    if results.is_empty() {
        return Ok(());
    }
    ensure_cache_table(conn)?;

    // DuckDB handles multi-row VALUES efficiently; chunk to stay within
    // reasonable parameter counts (4 params per row × 250 = 1000 params).
    const CHUNK_SIZE: usize = 250;
    for chunk in results.chunks(CHUNK_SIZE) {
        let row_placeholders: Vec<String> = chunk
            .iter()
            .map(|_| "(?, ?, ?, ?, current_timestamp)".to_string())
            .collect();
        let sql = format!(
            "INSERT OR REPLACE INTO geocode_cache (address, lat, lon, source, cached_at) VALUES {}",
            row_placeholders.join(", ")
        );
        let mut params_vec: Vec<Box<dyn duckdb::ToSql>> = Vec::with_capacity(chunk.len() * 4);
        for result in chunk {
            params_vec.push(Box::new(result.address.clone()));
            params_vec.push(Box::new(result.lat));
            params_vec.push(Box::new(result.lon));
            params_vec.push(Box::new(source.to_string()));
        }
        let params_refs: Vec<&dyn duckdb::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice())?;
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
