use serde::Deserialize;
use tracing::{debug, error, info, warn};

use crate::types::{GeoResult, GeocodeResult};

// ---- Nominatim API types ----
//
// Reference: https://nominatim.org/release-docs/develop/api/Search/
//
// The search endpoint (`GET /search?q={address}&format=jsonv2&limit=1`) returns
// an array of place objects.  We take the first result (highest relevance).
//
// Each result includes: lat, lon (strings!), importance (float 0-1),
// display_name, type, category, place_rank, etc.

#[derive(Debug, Deserialize)]
struct NominatimPlace {
    lat: String,
    lon: String,
    /// Relevance/importance score in [0, 1].  Used as confidence.
    #[serde(default)]
    importance: f64,
    #[serde(default)]
    #[allow(dead_code)]
    display_name: String,
}

/// Enriched result carrying Nominatim's importance score alongside coordinates.
pub(crate) struct NominatimEnrichedResult {
    pub(crate) inner: GeocodeResult,
    /// Nominatim importance score in [0, 1].
    pub(crate) importance: f64,
}

const DEFAULT_BASE_URL: &str = "https://nominatim.openstreetmap.org";
const USER_AGENT: &str = "Spatia/1.0 (https://github.com/spatia-app/spatia)";

/// Return the configured Nominatim base URL or the public instance default.
pub(crate) fn nominatim_base_url() -> String {
    std::env::var("SPATIA_NOMINATIM_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

/// Whether the configured base URL points to the public Nominatim instance
/// (requires 1-request-per-second rate limiting).
fn is_public_instance(base_url: &str) -> bool {
    let url = base_url.trim_end_matches('/').to_lowercase();
    url.contains("nominatim.openstreetmap.org")
}

/// Geocode a single address via the Nominatim search API.
///
/// Returns `Ok(None)` when Nominatim finds no results for the address.
pub(crate) async fn geocode_via_nominatim_single(
    client: &reqwest::Client,
    address: &str,
    base_url: &str,
) -> GeoResult<Option<NominatimEnrichedResult>> {
    let url = format!(
        "{}/search",
        base_url.trim_end_matches('/')
    );

    let resp = client
        .get(&url)
        .query(&[
            ("q", address),
            ("format", "jsonv2"),
            ("limit", "1"),
            ("addressdetails", "0"),
        ])
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .inspect_err(|e| {
            let kind = if e.is_timeout() {
                "timeout"
            } else if e.is_connect() {
                "connection"
            } else {
                "unknown"
            };
            error!(url = %url, error_kind = %kind, error = %e, "nominatim: request failed");
        })?;

    let status = resp.status();
    let body = resp.text().await?;

    if !status.is_success() {
        error!(url = %url, status = %status, body_preview = %&body[..body.len().min(200)], "nominatim: HTTP error");
        return Err(format!("Nominatim HTTP {status}").into());
    }

    let places: Vec<NominatimPlace> = serde_json::from_str(&body).map_err(|e| {
        error!(url = %url, error = %e, body_preview = %&body[..body.len().min(500)], "nominatim: JSON parse error");
        e
    })?;

    match places.into_iter().next() {
        Some(place) => {
            let lat: f64 = place.lat.parse().map_err(|e: std::num::ParseFloatError| {
                error!(lat = %place.lat, error = %e, "nominatim: invalid lat");
                e
            })?;
            let lon: f64 = place.lon.parse().map_err(|e: std::num::ParseFloatError| {
                error!(lon = %place.lon, error = %e, "nominatim: invalid lon");
                e
            })?;

            debug!(address = %address, lat, lon, importance = place.importance, "nominatim: resolved");

            Ok(Some(NominatimEnrichedResult {
                inner: GeocodeResult {
                    address: address.to_string(),
                    lat,
                    lon,
                    source: "nominatim".to_string(),
                },
                importance: place.importance,
            }))
        }
        None => {
            debug!(address = %address, "nominatim: no results");
            Ok(None)
        }
    }
}

/// Geocode a batch of addresses via Nominatim, one at a time with rate limiting.
///
/// Enforces a 1-second delay between requests for the public Nominatim instance.
/// Self-hosted instances (non-openstreetmap.org URLs) skip the delay.
///
/// The optional `progress_cb` is called after each address with (processed_count, total_count).
pub(crate) async fn geocode_via_nominatim_batch<F>(
    addresses: &[String],
    base_url: &str,
    progress_cb: Option<F>,
) -> GeoResult<Vec<NominatimEnrichedResult>>
where
    F: Fn(usize, usize),
{
    if addresses.is_empty() {
        return Ok(Vec::new());
    }

    let rate_limit = is_public_instance(base_url);
    if rate_limit {
        info!(
            count = addresses.len(),
            estimated_secs = addresses.len(),
            "nominatim: starting batch geocode (1 req/sec rate limit)"
        );
    } else {
        info!(
            count = addresses.len(),
            "nominatim: starting batch geocode (self-hosted, no rate limit)"
        );
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let mut results = Vec::new();
    let mut last_request = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_secs(2))
        .unwrap_or_else(std::time::Instant::now);

    for (i, address) in addresses.iter().enumerate() {
        // Enforce rate limit for public instance
        if rate_limit && i > 0 {
            let elapsed = last_request.elapsed();
            let min_interval = std::time::Duration::from_millis(1100); // slightly over 1s for safety
            if elapsed < min_interval {
                tokio::time::sleep(min_interval - elapsed).await;
            }
        }

        last_request = std::time::Instant::now();

        match geocode_via_nominatim_single(&client, address, base_url).await {
            Ok(Some(result)) => results.push(result),
            Ok(None) => {
                warn!(address = %address, index = i, "nominatim: unresolved");
            }
            Err(e) => {
                warn!(address = %address, index = i, error = %e, "nominatim: error (skipping)");
            }
        }

        if let Some(ref cb) = progress_cb {
            cb(i + 1, addresses.len());
        }
    }

    info!(resolved = results.len(), total = addresses.len(), "nominatim: batch complete");
    Ok(results)
}

/// Public wrapper matching the Geocodio API surface for backward compatibility.
pub async fn geocode_via_nominatim(
    addresses: &[String],
    base_url: &str,
) -> GeoResult<Vec<GeocodeResult>> {
    let enriched = geocode_via_nominatim_batch(addresses, base_url, None::<fn(usize, usize)>).await?;
    Ok(enriched.into_iter().map(|e| e.inner).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-N-001: Single address resolves correctly.
    #[tokio::test]
    async fn nominatim_single_resolves_address() {
        let mut server = mockito::Server::new_async().await;

        let fixture = r#"[{
            "lat": "47.6205",
            "lon": "-122.3493",
            "importance": 0.85,
            "display_name": "400 Broad St, Seattle, WA 98109, USA",
            "type": "house",
            "category": "place"
        }]"#;

        let _mock = server
            .mock("GET", "/search")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("q".into(), "400 Broad St, Seattle, WA 98109".into()),
                mockito::Matcher::UrlEncoded("format".into(), "jsonv2".into()),
                mockito::Matcher::UrlEncoded("limit".into(), "1".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(fixture)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let result = geocode_via_nominatim_single(
            &client,
            "400 Broad St, Seattle, WA 98109",
            &server.url(),
        )
        .await
        .expect("should resolve");

        let result = result.expect("should have a result");
        assert!((result.inner.lat - 47.6205).abs() < 1e-4);
        assert!((result.inner.lon - (-122.3493)).abs() < 1e-4);
        assert_eq!(result.inner.source, "nominatim");
        assert!((result.importance - 0.85).abs() < 1e-6);
    }

    /// TC-N-002: Empty results array returns None.
    #[tokio::test]
    async fn nominatim_single_no_results_returns_none() {
        let mut server = mockito::Server::new_async().await;

        let _mock = server
            .mock("GET", "/search")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("[]")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let result = geocode_via_nominatim_single(
            &client,
            "nonexistent address xyz123",
            &server.url(),
        )
        .await
        .expect("should not error");

        assert!(result.is_none());
    }

    /// TC-N-003: Empty batch returns empty immediately.
    #[tokio::test]
    async fn nominatim_batch_empty_returns_empty() {
        let results = geocode_via_nominatim_batch(
            &[],
            "http://unused",
            None::<fn(usize, usize)>,
        )
        .await
        .expect("empty batch should succeed");

        assert!(results.is_empty());
    }

    /// TC-N-004: Batch processes multiple addresses and calls progress callback.
    #[tokio::test]
    async fn nominatim_batch_calls_progress() {
        let mut server = mockito::Server::new_async().await;

        let fixture = r#"[{
            "lat": "39.7817",
            "lon": "-89.6501",
            "importance": 0.7,
            "display_name": "123 Main St, Springfield, IL"
        }]"#;

        let _mock = server
            .mock("GET", "/search")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(fixture)
            .expect(2)
            .create_async()
            .await;

        let progress = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let progress_clone = progress.clone();

        let addresses = vec![
            "addr 1".to_string(),
            "addr 2".to_string(),
        ];

        let results = geocode_via_nominatim_batch(
            &addresses,
            &server.url(), // not public instance → no rate limit delay
            Some(move |done: usize, total: usize| {
                progress_clone.lock().unwrap().push((done, total));
            }),
        )
        .await
        .expect("batch should succeed");

        assert_eq!(results.len(), 2);
        let calls = progress.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], (1, 2));
        assert_eq!(calls[1], (2, 2));
    }

    /// TC-N-005: HTTP error returns Err, not panic.
    #[tokio::test]
    async fn nominatim_single_http_error() {
        let mut server = mockito::Server::new_async().await;

        let _mock = server
            .mock("GET", "/search")
            .match_query(mockito::Matcher::Any)
            .with_status(429)
            .with_body("Too Many Requests")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let result = geocode_via_nominatim_single(
            &client,
            "123 Main St",
            &server.url(),
        )
        .await;

        assert!(result.is_err());
    }

    /// TC-N-006: Malformed JSON returns error.
    #[tokio::test]
    async fn nominatim_single_malformed_json() {
        let mut server = mockito::Server::new_async().await;

        let _mock = server
            .mock("GET", "/search")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("not json {{{")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let result = geocode_via_nominatim_single(
            &client,
            "123 Main St",
            &server.url(),
        )
        .await;

        assert!(result.is_err());
    }

    /// TC-N-007: Public instance detection.
    #[test]
    fn public_instance_detection() {
        assert!(is_public_instance("https://nominatim.openstreetmap.org"));
        assert!(is_public_instance("https://nominatim.openstreetmap.org/"));
        assert!(is_public_instance("http://nominatim.openstreetmap.org"));
        assert!(!is_public_instance("http://localhost:8080"));
        assert!(!is_public_instance("https://my-nominatim.example.com"));
    }

    /// TC-N-008: Public wrapper returns GeocodeResult vec.
    #[tokio::test]
    async fn nominatim_public_wrapper() {
        let mut server = mockito::Server::new_async().await;

        let fixture = r#"[{
            "lat": "41.8781",
            "lon": "-87.6298",
            "importance": 0.9,
            "display_name": "Chicago, IL"
        }]"#;

        let _mock = server
            .mock("GET", "/search")
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(fixture)
            .create_async()
            .await;

        let results = geocode_via_nominatim(
            &["Chicago, IL".to_string()],
            &server.url(),
        )
        .await
        .expect("should succeed");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, "nominatim");
        assert!((results[0].lat - 41.8781).abs() < 1e-4);
    }

    /// TC-N-009: Batch skips errors and continues.
    #[tokio::test]
    async fn nominatim_batch_skips_errors() {
        let mut server = mockito::Server::new_async().await;

        // First request succeeds, second fails with 500, third succeeds
        let good_fixture = r#"[{"lat":"40.7128","lon":"-74.0060","importance":0.8,"display_name":"NYC"}]"#;

        // mockito doesn't easily support different responses per call,
        // so we test with all-succeed or all-fail patterns.
        let _mock = server
            .mock("GET", "/search")
            .match_query(mockito::Matcher::Any)
            .with_status(500)
            .with_body("Internal Server Error")
            .expect(2)
            .create_async()
            .await;

        let addresses = vec!["addr1".to_string(), "addr2".to_string()];
        let results = geocode_via_nominatim_batch(
            &addresses,
            &server.url(),
            None::<fn(usize, usize)>,
        )
        .await
        .expect("batch should not error even if individual calls fail");

        // All failed → empty results (not an error)
        assert_eq!(results.len(), 0);
    }
}
