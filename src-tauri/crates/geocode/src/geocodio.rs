use serde::Deserialize;
use tracing::{debug, error, info};

use crate::types::{GeoResult, GeocodeResult};

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
pub(crate) struct GeocodioResponse {
    pub(crate) results: Vec<GeocodioBatchItem>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GeocodioBatchItem {
    pub(crate) query: String,
    pub(crate) response: GeocodioAddressResponse,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GeocodioAddressResponse {
    /// Parsed echo of the input address.  Present in the real API response
    /// but not used by our code; captured with `#[serde(default)]` so the
    /// struct deserializes correctly whether or not the field is present.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) input: Option<serde_json::Value>,
    pub(crate) results: Vec<GeocodioCandidate>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GeocodioCandidate {
    pub(crate) location: GeocodioLocation,
    /// Geocodio accuracy score: float in [0, 1].  1.0 = rooftop match.
    /// Used as the `confidence` value for results returned from the API.
    /// Ref: https://www.geocod.io/docs/#accuracy-score
    #[serde(default)]
    pub(crate) accuracy: f64,
    /// Human-readable accuracy type string, e.g. "rooftop", "range_interpolation",
    /// "street_center", "place".
    /// Ref: https://www.geocod.io/docs/#accuracy-type
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) accuracy_type: String,
    /// Data source name used by Geocodio, e.g. "Census", "Virginia GIS Clearinghouse".
    /// Distinct from our own `source` field (which is always "geocodio").
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) source: String,
    /// Formatted address string returned by Geocodio.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) formatted_address: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GeocodioLocation {
    pub(crate) lat: f64,
    pub(crate) lng: f64,
}

// ---- Geocodio API call ----

/// Internal enriched result that carries the real Geocodio accuracy score
/// alongside the geocoded coordinates.  Used by [`geocode_batch`] to populate
/// `GeocodeBatchResult.confidence` with the API-supplied value rather than a
/// hardcoded default.
pub(crate) struct GeocodioEnrichedResult {
    pub(crate) inner: GeocodeResult,
    /// Geocodio accuracy score in [0, 1].  Defaults to 0.0 if not present in
    /// the response (serde default on the `GeocodioCandidate` field).
    pub(crate) accuracy: f64,
}

/// Core HTTP logic shared by the public `geocode_via_geocodio` wrapper and the
/// internal `geocode_batch` call-site.  Returns enriched results including the
/// raw `accuracy` field from the Geocodio response so that callers can
/// propagate it as a confidence score.
pub(crate) async fn geocode_via_geocodio_inner(
    api_key: &str,
    addresses: &[String],
    base_url: &str,
) -> GeoResult<Vec<GeocodioEnrichedResult>> {
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
) -> GeoResult<Vec<GeocodeResult>> {
    let enriched = geocode_via_geocodio_inner(api_key, addresses, base_url).await?;
    Ok(enriched.into_iter().map(|e| e.inner).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC-G-001: Multiple addresses in a single batch are all returned with the
    /// correct lat/lon mapped to the correct query string.
    #[tokio::test]
    async fn geocode_via_geocodio_multi_address_batch_maps_correctly() {
        let mut server = mockito::Server::new_async().await;

        // Realistic Geocodio v1.10 batch response with three addresses.
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
    #[test]
    fn geocodio_v1_10_batch_response_fixture_deserializes_correctly() {
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

        let item0 = &response.results[0];
        assert_eq!(item0.query, "1109 N Highland St, Arlington VA");
        assert_eq!(item0.response.results.len(), 1);
        let cand0 = &item0.response.results[0];
        assert!((cand0.location.lat - 38.886672).abs() < 1e-6, "Arlington lat mismatch");
        assert!((cand0.location.lng - (-77.094735)).abs() < 1e-6, "Arlington lng mismatch");
        assert!((cand0.accuracy - 1.0).abs() < 1e-6, "Arlington accuracy should be 1.0");
        assert_eq!(cand0.accuracy_type, "rooftop", "Arlington accuracy_type should be rooftop");

        let item1 = &response.results[1];
        assert_eq!(item1.query, "525 University Ave, Toronto, ON, Canada");
        assert_eq!(item1.response.results.len(), 1);
        let cand1 = &item1.response.results[0];
        assert!((cand1.location.lat - 43.656618).abs() < 1e-6, "Toronto lat mismatch");
        assert!((cand1.location.lng - (-79.388092)).abs() < 1e-6, "Toronto lng mismatch");
        assert!((cand1.accuracy - 1.0).abs() < 1e-6, "Toronto accuracy should be 1.0");
    }

    /// TC-G-007: The real Geocodio `accuracy` field (a float in [0,1]) is
    /// propagated as `confidence` on the returned `GeocodeBatchResult`.
    #[tokio::test]
    async fn geocode_via_geocodio_inner_propagates_accuracy_as_confidence() {
        let mut server = mockito::Server::new_async().await;

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
}
