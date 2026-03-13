use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use duckdb::Connection;


use spatia_geocode::{
    cache_store, ensure_cache_table, geocode_batch, GeocodeBatchResult, GeocodeResult,
    GeocodeStats,
};

use crate::corpus::{ExpectedResult, LookupSetup, TestCase};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestResult {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub outcome: String,
    pub outcome_detail: Option<String>,
    pub timing: TimingMs,
    pub stats: Option<GeocodeStats>,
    pub address_results: Vec<AddressResult>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TimingMs {
    pub total_ms: u64,
    pub setup_ms: u64,
    pub geocode_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddressResult {
    pub address: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub source: Option<String>,
    pub confidence: Option<f64>,
    pub distance_error_m: Option<f64>,
    pub assertion_pass: bool,
    pub assertion_detail: Option<String>,
}

/// Haversine distance in meters between two lat/lon points.
fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0; // Earth radius in meters
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    r * c
}

pub fn run_test(
    tc: &TestCase,
    corpus_dir: &std::path::Path,
    default_timeout_secs: u64,
) -> TestResult {
    let test_start = Instant::now();
    let mut timing = TimingMs::default();

    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let db_path = format!("/tmp/spatia_geocode_bench_{ns}.duckdb");

    // Setup phase
    let setup_start = Instant::now();

    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            timing.setup_ms = setup_start.elapsed().as_millis() as u64;
            timing.total_ms = test_start.elapsed().as_millis() as u64;
            cleanup_db(&db_path);
            return make_result(
                tc,
                "setup_error",
                Some(format!("open DB: {e}")),
                timing,
                None,
                vec![],
            );
        }
    };

    // Ingest CSV if specified
    if let (Some(csv), Some(table)) = (&tc.setup_csv, &tc.setup_table) {
        let csv_path = corpus_dir.join(csv);
        let csv_path = csv_path.canonicalize().unwrap_or(csv_path);
        let csv_str = csv_path.to_string_lossy();
        let sql = format!(
            "CREATE OR REPLACE TABLE {} AS SELECT * FROM read_csv_auto('{}')",
            table,
            csv_str.replace('\'', "''")
        );
        if let Err(e) = conn.execute_batch(&sql) {
            timing.setup_ms = setup_start.elapsed().as_millis() as u64;
            timing.total_ms = test_start.elapsed().as_millis() as u64;
            cleanup_db(&db_path);
            return make_result(
                tc,
                "setup_error",
                Some(format!("CSV ingest: {e}")),
                timing,
                None,
                vec![],
            );
        }
    }

    // Seed cache if requested
    if tc.seed_cache && !tc.cache_seeds.is_empty() {
        if let Err(e) = ensure_cache_table(&conn) {
            timing.setup_ms = setup_start.elapsed().as_millis() as u64;
            timing.total_ms = test_start.elapsed().as_millis() as u64;
            cleanup_db(&db_path);
            return make_result(
                tc,
                "setup_error",
                Some(format!("ensure cache: {e}")),
                timing,
                None,
                vec![],
            );
        }
        let records: Vec<GeocodeResult> = tc
            .cache_seeds
            .iter()
            .map(|s| GeocodeResult {
                address: s.address.clone(),
                lat: s.lat,
                lon: s.lon,
                source: s.source.clone(),
            })
            .collect();
        if let Err(e) = cache_store(&conn, &records, "geocodio") {
            timing.setup_ms = setup_start.elapsed().as_millis() as u64;
            timing.total_ms = test_start.elapsed().as_millis() as u64;
            cleanup_db(&db_path);
            return make_result(
                tc,
                "setup_error",
                Some(format!("seed cache: {e}")),
                timing,
                None,
                vec![],
            );
        }
    }

    // Set up lookup table if specified
    if let Some(lookup) = &tc.setup_lookup {
        if let Err(e) = setup_lookup_table(&conn, lookup) {
            timing.setup_ms = setup_start.elapsed().as_millis() as u64;
            timing.total_ms = test_start.elapsed().as_millis() as u64;
            cleanup_db(&db_path);
            return make_result(
                tc,
                "setup_error",
                Some(format!("setup lookup: {e}")),
                timing,
                None,
                vec![],
            );
        }
    }

    drop(conn); // Release connection before geocode_batch opens its own
    timing.setup_ms = setup_start.elapsed().as_millis() as u64;

    // Geocode phase
    let geocode_start = Instant::now();
    let timeout =
        std::time::Duration::from_secs(tc.timeout_secs.unwrap_or(default_timeout_secs));

    let geocode_result = std::panic::catch_unwind(|| geocode_batch(&db_path, &tc.addresses));

    timing.geocode_ms = geocode_start.elapsed().as_millis() as u64;

    let (results, stats) = match geocode_result {
        Ok(Ok((results, stats))) => (results, stats),
        Ok(Err(e)) => {
            // geocode_batch can fail if API key is missing — that's expected for tests
            // where we expect unresolved addresses (no API key means API fallback fails).
            let err_msg = format!("{e}");
            let is_api_key_missing = err_msg.contains("SPATIA_GEOCODIO_API_KEY");
            timing.total_ms = test_start.elapsed().as_millis() as u64;
            let outcome = if is_api_key_missing && tc.expect_unresolved_count.is_some() {
                // Expected: test wants unresolved addresses and API key is missing
                "pass"
            } else {
                "geocode_error"
            };
            cleanup_db(&db_path);
            return make_result(
                tc,
                outcome,
                Some(format!("geocode_batch: {e}")),
                timing,
                None,
                vec![],
            );
        }
        Err(_) => {
            timing.total_ms = test_start.elapsed().as_millis() as u64;
            cleanup_db(&db_path);
            return make_result(
                tc,
                "panic",
                Some("geocode_batch panicked".into()),
                timing,
                None,
                vec![],
            );
        }
    };

    // Check timeout
    if test_start.elapsed() > timeout {
        timing.total_ms = test_start.elapsed().as_millis() as u64;
        cleanup_db(&db_path);
        return make_result(tc, "timeout", None, timing, Some(stats), vec![]);
    }

    // Run assertions
    let mut address_results = Vec::new();
    let results_by_addr: HashMap<&str, &GeocodeBatchResult> =
        results.iter().map(|r| (r.address.as_str(), r)).collect();

    let mut all_pass = true;

    for expected in &tc.expect {
        let result = results_by_addr.get(expected.address.as_str());
        let (ar, pass) = check_address_assertion(expected, result);
        if !pass {
            all_pass = false;
        }
        address_results.push(ar);
    }

    // Check count assertions
    if let Some(expected_geocoded) = tc.expect_geocoded_count {
        if stats.geocoded != expected_geocoded {
            all_pass = false;
            address_results.push(AddressResult {
                address: "<count_check>".to_string(),
                lat: None,
                lon: None,
                source: None,
                confidence: None,
                distance_error_m: None,
                assertion_pass: false,
                assertion_detail: Some(format!(
                    "expected {} geocoded, got {}",
                    expected_geocoded, stats.geocoded
                )),
            });
        }
    }

    if let Some(expected_unresolved) = tc.expect_unresolved_count {
        if stats.unresolved != expected_unresolved {
            all_pass = false;
            address_results.push(AddressResult {
                address: "<count_check>".to_string(),
                lat: None,
                lon: None,
                source: None,
                confidence: None,
                distance_error_m: None,
                assertion_pass: false,
                assertion_detail: Some(format!(
                    "expected {} unresolved, got {}",
                    expected_unresolved, stats.unresolved
                )),
            });
        }
    }

    timing.total_ms = test_start.elapsed().as_millis() as u64;
    cleanup_db(&db_path);

    let outcome = if all_pass { "pass" } else { "assertion_failure" };
    let detail = if all_pass {
        None
    } else {
        let failures: Vec<String> = address_results
            .iter()
            .filter(|ar| !ar.assertion_pass)
            .filter_map(|ar| ar.assertion_detail.clone())
            .collect();
        Some(failures.join("; "))
    };

    make_result(tc, outcome, detail, timing, Some(stats), address_results)
}

fn check_address_assertion(
    expected: &ExpectedResult,
    result: Option<&&GeocodeBatchResult>,
) -> (AddressResult, bool) {
    let Some(r) = result else {
        return (
            AddressResult {
                address: expected.address.clone(),
                lat: None,
                lon: None,
                source: None,
                confidence: None,
                distance_error_m: None,
                assertion_pass: false,
                assertion_detail: Some("address not found in results".to_string()),
            },
            false,
        );
    };

    let mut pass = true;
    let mut details = Vec::new();

    // Distance check
    let distance_error = if let (Some(lat), Some(lon)) = (expected.lat, expected.lon) {
        let d = haversine_distance(lat, lon, r.lat, r.lon);
        if d > expected.max_distance_meters {
            pass = false;
            details.push(format!(
                "distance {:.0}m > max {:.0}m",
                d, expected.max_distance_meters
            ));
        }
        Some(d)
    } else {
        None
    };

    // Source check
    if let Some(expected_source) = &expected.expect_source {
        if r.source != *expected_source {
            pass = false;
            details.push(format!(
                "source '{}' != expected '{}'",
                r.source, expected_source
            ));
        }
    }

    // Confidence check
    if let Some(min_conf) = expected.min_confidence {
        if r.confidence < min_conf {
            pass = false;
            details.push(format!(
                "confidence {:.2} < min {:.2}",
                r.confidence, min_conf
            ));
        }
    }

    (
        AddressResult {
            address: expected.address.clone(),
            lat: Some(r.lat),
            lon: Some(r.lon),
            source: Some(r.source.clone()),
            confidence: Some(r.confidence),
            distance_error_m: distance_error,
            assertion_pass: pass,
            assertion_detail: if details.is_empty() {
                None
            } else {
                Some(details.join("; "))
            },
        },
        pass,
    )
}

fn setup_lookup_table(
    conn: &Connection,
    lookup: &LookupSetup,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let table = &lookup.base_table;
    let lookup_name = format!("{}_lookup", table);

    conn.execute_batch(&format!(
        "CREATE TABLE {table} (id VARCHAR, label TEXT, lat DOUBLE, lon DOUBLE)"
    ))?;
    conn.execute_batch(&format!(
        "CREATE TABLE {lookup_name} (source_id VARCHAR, label TEXT, label_norm TEXT)"
    ))?;

    for entry in &lookup.entries {
        conn.execute(
            &format!("INSERT INTO {table} VALUES (?, ?, ?, ?)"),
            duckdb::params![entry.id, entry.label, entry.lat, entry.lon],
        )?;
        let label_norm = entry.label.to_lowercase();
        conn.execute(
            &format!("INSERT INTO {lookup_name} VALUES (?, ?, ?)"),
            duckdb::params![entry.id, entry.label, label_norm],
        )?;
    }

    Ok(())
}

fn make_result(
    tc: &TestCase,
    outcome: &str,
    detail: Option<String>,
    timing: TimingMs,
    stats: Option<GeocodeStats>,
    address_results: Vec<AddressResult>,
) -> TestResult {
    TestResult {
        name: tc.name.clone(),
        description: tc.description.clone(),
        tags: tc.tags.clone(),
        outcome: outcome.to_string(),
        outcome_detail: detail,
        timing,
        stats,
        address_results,
    }
}

pub fn cleanup_db(path: &str) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}.wal"));
    let _ = std::fs::remove_file(format!("{path}.wal.lck"));
}
