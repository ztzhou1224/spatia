use serde::Deserialize;

use crate::{EngineResult, GeocodeResult};

const DEFAULT_BATCH_SIZE: usize = 100;
const GEOCODIO_BASE_URL: &str = "https://api.geocodio.com/v1.7";

/// Top-level Geocodio batch response shape.
#[derive(Debug, Deserialize)]
struct GeocodioTopLevel {
    results: Vec<GeocodioEntry>,
}

/// Per-address entry in the Geocodio batch response.
#[derive(Debug, Deserialize)]
struct GeocodioEntry {
    query: String,
    response: GeocodioResponse,
}

#[derive(Debug, Deserialize)]
struct GeocodioResponse {
    results: Vec<GeocodioMatch>,
}

#[derive(Debug, Deserialize)]
struct GeocodioMatch {
    location: GeocodioLocation,
}

#[derive(Debug, Deserialize)]
struct GeocodioLocation {
    lat: f64,
    lng: f64,
}

/// Call the Geocodio batch geocoding API for the given addresses.
///
/// Requires the `SPATIA_GEOCODIO_API_KEY` env var to be set.  Splits large
/// batches according to `SPATIA_GEOCODIO_BATCH_SIZE` (default 100).
pub fn geocode_via_geocodio(addresses: &[String]) -> EngineResult<Vec<GeocodeResult>> {
    let api_key = std::env::var("SPATIA_GEOCODIO_API_KEY")
        .map_err(|_| "SPATIA_GEOCODIO_API_KEY env var is not set")?;
    if api_key.trim().is_empty() {
        return Err("SPATIA_GEOCODIO_API_KEY env var is empty".into());
    }

    let batch_size = std::env::var("SPATIA_GEOCODIO_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_BATCH_SIZE);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(geocode_geocodio_async(addresses, &api_key, batch_size))
}

async fn geocode_geocodio_async(
    addresses: &[String],
    api_key: &str,
    batch_size: usize,
) -> EngineResult<Vec<GeocodeResult>> {
    let client = reqwest::Client::new();
    // Geocodio requires the API key as a query parameter (their documented pattern).
    // Avoid logging this URL to prevent accidental key leakage.
    let url = format!("{GEOCODIO_BASE_URL}/geocode?api_key={api_key}&limit=1");

    let mut all_results: Vec<GeocodeResult> = Vec::with_capacity(addresses.len());

    for chunk in addresses.chunks(batch_size) {
        let response = client
            .post(&url)
            .json(&chunk)
            .send()
            .await?
            .error_for_status()?;

        let top: GeocodioTopLevel = response.json().await?;

        for entry in top.results {
            let location = entry
                .response
                .results
                .into_iter()
                .next()
                .map(|m| (m.location.lat, m.location.lng));

            all_results.push(GeocodeResult {
                address: entry.query,
                lat: location.map(|(lat, _)| lat),
                lon: location.map(|(_, lng)| lng),
                status: None,
                error: None,
            });
        }
    }

    Ok(all_results)
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_BATCH_SIZE;

    #[test]
    fn default_batch_size_is_sensible() {
        assert!(DEFAULT_BATCH_SIZE > 0);
        assert!(DEFAULT_BATCH_SIZE <= 10_000);
    }

    #[test]
    fn geocodio_returns_err_without_api_key() {
        std::env::remove_var("SPATIA_GEOCODIO_API_KEY");
        let err = super::geocode_via_geocodio(&["123 Main St".to_string()])
            .expect_err("should fail without key");
        assert!(
            err.to_string().contains("SPATIA_GEOCODIO_API_KEY"),
            "error should mention env var: {err}"
        );
    }
}
