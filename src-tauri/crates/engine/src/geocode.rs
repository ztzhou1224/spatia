use duckdb::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::EngineResult;

/// A geocoded address result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeocodeResult {
    pub address: String,
    pub lat: f64,
    pub lon: f64,
    pub source: String,
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

// ---- Geocodio API types ----

#[derive(Debug, Deserialize)]
struct GeocodioResponse {
    results: HashMap<String, GeocodioAddressResult>,
}

#[derive(Debug, Deserialize)]
struct GeocodioAddressResult {
    results: Vec<GeocodioCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeocodioCandidate {
    location: GeocodioLocation,
}

#[derive(Debug, Deserialize)]
struct GeocodioLocation {
    lat: f64,
    lng: f64,
}

// ---- Geocodio API call ----

/// Call the Geocodio batch geocoding endpoint.
///
/// `base_url` should be `"https://api.geocodio.com"` in production.
/// It is accepted as a parameter to allow test overriding.
pub async fn geocode_via_geocodio(
    api_key: &str,
    addresses: &[String],
    base_url: &str,
) -> EngineResult<Vec<GeocodeResult>> {
    let batch_size: usize = std::env::var("SPATIA_GEOCODIO_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(100)
        .clamp(1, 10_000);

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1.7/geocode?api_key={}",
        base_url.trim_end_matches('/'),
        api_key
    );

    let mut results = Vec::new();

    for chunk in addresses.chunks(batch_size) {
        let response: GeocodioResponse = client
            .post(&url)
            .json(chunk)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        for address in chunk {
            if let Some(addr_result) = response.results.get(address) {
                if let Some(candidate) = addr_result.results.first() {
                    results.push(GeocodeResult {
                        address: address.clone(),
                        lat: candidate.location.lat,
                        lon: candidate.location.lng,
                        source: "geocodio".to_string(),
                    });
                }
            }
        }
    }

    Ok(results)
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
pub fn geocode_addresses(db_path: &str, addresses: &[String]) -> EngineResult<Vec<GeocodeResult>> {
    let conn = Connection::open(db_path)?;

    let (mut results, misses) = cache_lookup(&conn, addresses)?;

    if !misses.is_empty() {
        let api_key = std::env::var("SPATIA_GEOCODIO_API_KEY")
            .map_err(|_| "SPATIA_GEOCODIO_API_KEY environment variable not set")?;
        let base_url = std::env::var("SPATIA_GEOCODIO_BASE_URL")
            .unwrap_or_else(|_| "https://api.geocodio.com".to_string());

        let new_results = run_async(geocode_via_geocodio(&api_key, &misses, &base_url))?;
        cache_store(&conn, &new_results, "geocodio")?;
        results.extend(new_results);
    }

    Ok(results)
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
            "results": {
                "123 Main St, Springfield, IL": {
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
        }"#;

        let _mock = server
            .mock("POST", "/v1.7/geocode?api_key=test_key")
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
}
