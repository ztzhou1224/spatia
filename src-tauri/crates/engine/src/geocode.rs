use serde::{Deserialize, Serialize};

use crate::geocode_cache::{cache_lookup, cache_store, ensure_cache_table};
use crate::geocodio::geocode_via_geocodio;
use crate::EngineResult;

pub const DEFAULT_GEOCODER_URL: &str = "http://127.0.0.1:7788";

#[derive(Debug, Clone, Serialize)]
struct GeocodeRequest {
    addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeocodeResult {
    pub address: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Geocode a batch of addresses using a cache-first strategy:
///
/// 1. If `db_path` is `Some`, check `geocode_cache` in DuckDB and return cached
///    coordinates for matching addresses.
/// 2. For the remaining addresses, call the Geocodio API as a fallback
///    (requires `SPATIA_GEOCODIO_API_KEY`).
/// 3. Write all newly resolved results back to the cache.
pub fn geocode_batch_hybrid(
    addresses: &[String],
    db_path: Option<&str>,
) -> EngineResult<Vec<GeocodeResult>> {
    // --- Step 1: cache lookup ---
    let (mut resolved, to_geocode) = if let Some(path) = db_path {
        let conn = duckdb::Connection::open(path)?;
        ensure_cache_table(&conn)?;
        let (hits, misses) = cache_lookup(&conn, addresses)?;
        (hits, misses)
    } else {
        (vec![], addresses.to_vec())
    };

    if to_geocode.is_empty() {
        return Ok(resolved);
    }

    // --- Step 2: Geocodio fallback ---
    if std::env::var("SPATIA_GEOCODIO_API_KEY").is_ok() {
        // Geocodio errors are intentionally swallowed: callers receive null-coord
        // entries rather than a hard error when the external service is unavailable.
        let geocodio_results = geocode_via_geocodio(&to_geocode).unwrap_or_default();

        if let Some(path) = db_path {
            // Cache failures are non-fatal.
            if !geocodio_results.is_empty() {
                if let Ok(conn) = duckdb::Connection::open(path) {
                    let _ = cache_store(&conn, &geocodio_results, "geocodio");
                }
            }
        }

        resolved.extend(geocodio_results);
    } else {
        // No Geocodio key — propagate unresolved addresses as null-coord entries.
        for address in to_geocode {
            resolved.push(GeocodeResult {
                address,
                lat: None,
                lon: None,
                status: None,
                error: None,
            });
        }
    }

    Ok(resolved)
}

pub async fn geocode_batch(
    base_url: &str,
    addresses: &[String],
) -> EngineResult<Vec<GeocodeResult>> {
    let trimmed = base_url.trim_end_matches('/');
    let url = format!("{trimmed}/geocode");
    let client = reqwest::Client::new();
    let payload = GeocodeRequest {
        addresses: addresses.to_vec(),
    };
    let response = client.post(url).json(&payload).send().await?;
    let response = response.error_for_status()?;
    let results = response.json::<Vec<GeocodeResult>>().await?;
    Ok(results)
}

