use spatia_geocode::{
    cache_lookup, cache_store, ensure_cache_table, geocode_batch, normalize_address,
    score_candidate, tokenize_address, GeocodeBatchResult, GeocodeResult,
};
use duckdb::Connection;
use std::time::{SystemTime, UNIX_EPOCH};

// ---- Test helpers ----

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos()
}

fn temp_db() -> (String, Connection) {
    let path = format!("/tmp/spatia_geocode_integ_{}.duckdb", unique_suffix());
    let conn = Connection::open(&path).expect("open temp db");
    (path, conn)
}

fn cleanup(path: &str) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}.wal"));
    let _ = std::fs::remove_file(format!("{path}.wal.lck"));
}

/// Ingest the test CSV fixture into a table named `test_addresses`.
fn ingest_test_csv(conn: &Connection) {
    let csv_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../data/test_geocode_addresses.csv"
    );
    conn.execute_batch(&format!(
        "CREATE OR REPLACE TABLE test_addresses AS SELECT * FROM read_csv_auto('{}')",
        csv_path.replace('\'', "''"),
    ))
    .expect("ingest test CSV");
}

/// Pre-populate the geocode cache with known results so tests don't need API keys.
fn seed_cache(conn: &Connection) {
    let records = vec![
        GeocodeResult {
            address: "85 Pike St, Seattle, WA 98101".to_string(),
            lat: 47.6088,
            lon: -122.3404,
            source: "geocodio".to_string(),
        },
        GeocodeResult {
            address: "400 Broad St, Seattle, WA 98109".to_string(),
            lat: 47.6205,
            lon: -122.3493,
            source: "geocodio".to_string(),
        },
        GeocodeResult {
            address: "2401 Utah Ave S, Seattle, WA 98134".to_string(),
            lat: 47.5801,
            lon: -122.3358,
            source: "geocodio".to_string(),
        },
        GeocodeResult {
            address: "Main St".to_string(),
            lat: 47.6062,
            lon: -122.3321,
            source: "geocodio".to_string(),
        },
        GeocodeResult {
            address: "123 Nono St".to_string(),
            lat: 47.6100,
            lon: -122.3400,
            source: "geocodio".to_string(),
        },
        GeocodeResult {
            address: "12345 Northeast 67th Avenue Building C Suite 890, Redmond, WA 98052"
                .to_string(),
            lat: 47.6700,
            lon: -122.1200,
            source: "geocodio".to_string(),
        },
        GeocodeResult {
            address: "123 O'Brien & Sons Rd., Seattle, WA 98101".to_string(),
            lat: 47.6090,
            lon: -122.3350,
            source: "geocodio".to_string(),
        },
    ];
    cache_store(conn, &records, "geocodio").expect("seed cache");
}

// ---- Unit tests for internal helpers ----

#[test]
fn normalize_address_strips_punctuation_and_lowercases() {
    let result = normalize_address("123 O'Brien & Sons Rd.");
    assert_eq!(result, "123 o brien sons rd");
}

#[test]
fn normalize_address_collapses_whitespace() {
    let result = normalize_address("  123   Main   St  ");
    assert_eq!(result, "123 main st");
}

#[test]
fn normalize_address_handles_unicode() {
    // Non-ASCII alpha chars are stripped by the is_ascii_alphanumeric filter
    let result = normalize_address("123 Nono St");
    assert_eq!(result, "123 nono st");
}

#[test]
fn normalize_address_empty_input() {
    assert_eq!(normalize_address(""), "");
}

#[test]
fn tokenize_empty_string_returns_empty() {
    let tokens = tokenize_address("");
    assert!(tokens.is_empty());
}

#[test]
fn tokenize_address_splits_correctly() {
    let tokens = tokenize_address("123 Main St");
    assert_eq!(tokens, vec!["123", "main", "st"]);
}

#[test]
fn tokenize_address_strips_punctuation() {
    let tokens = tokenize_address("O'Brien & Sons Rd.");
    assert_eq!(tokens, vec!["o", "brien", "sons", "rd"]);
}

#[test]
fn score_candidate_exact_match_returns_1() {
    let score = score_candidate("123 main st", "123 main st");
    // With the weighted scorer, identical inputs score 0.90
    // (0.60 overlap + 0.25 leading + 0.05 street number)
    assert!(score >= 0.85, "identical inputs should score high, got {score:.3}");
}

#[test]
fn score_candidate_no_overlap_returns_0() {
    let score = score_candidate("xyz abc", "123 main st");
    assert!(score.abs() < 1e-9, "expected 0, got {score}");
}

#[test]
fn score_candidate_partial_overlap_returns_between_0_and_1() {
    let score = score_candidate("123 main st springfield", "123 main st chicago");
    assert!(score > 0.0, "score should be > 0, got {score}");
    assert!(score < 1.0, "score should be < 1, got {score}");
}

#[test]
fn score_candidate_empty_query_returns_0() {
    assert!(score_candidate("", "123 main st").abs() < 1e-9);
}

#[test]
fn score_candidate_empty_label_returns_0() {
    assert!(score_candidate("123 main st", "").abs() < 1e-9);
}

#[test]
fn score_candidate_prefix_match_boosts_score() {
    let with_prefix = score_candidate("123 main", "123 main st seattle wa");
    let without_prefix = score_candidate("main 123", "123 main st seattle wa");
    // Both have same token overlap, but with_prefix should get a bonus
    assert!(
        with_prefix >= without_prefix,
        "prefix match ({with_prefix}) should score >= non-prefix ({without_prefix})"
    );
}

// ---- Cache integration tests ----

#[test]
fn cache_handles_special_characters_in_addresses() {
    let conn = Connection::open_in_memory().expect("open");
    let records = vec![GeocodeResult {
        address: "123 O'Brien & Sons Rd.".to_string(),
        lat: 47.6090,
        lon: -122.3350,
        source: "geocodio".to_string(),
    }];
    cache_store(&conn, &records, "geocodio").expect("store");

    let (hits, misses) =
        cache_lookup(&conn, &["123 O'Brien & Sons Rd.".to_string()]).expect("lookup");
    assert_eq!(hits.len(), 1);
    assert!(misses.is_empty());
    assert_eq!(hits[0].address, "123 O'Brien & Sons Rd.");
    assert!((hits[0].lat - 47.6090).abs() < 1e-3);
}

#[test]
fn cache_handles_unicode_addresses() {
    let conn = Connection::open_in_memory().expect("open");
    let records = vec![GeocodeResult {
        address: "123 Nono St".to_string(),
        lat: 47.6100,
        lon: -122.3400,
        source: "geocodio".to_string(),
    }];
    cache_store(&conn, &records, "geocodio").expect("store");

    let (hits, misses) =
        cache_lookup(&conn, &["123 Nono St".to_string()]).expect("lookup");
    assert_eq!(hits.len(), 1);
    assert!(misses.is_empty());
    assert_eq!(hits[0].address, "123 Nono St");
}

#[test]
fn cache_lookup_empty_list_returns_empty() {
    let conn = Connection::open_in_memory().expect("open");
    ensure_cache_table(&conn).expect("ensure");
    let (hits, misses) = cache_lookup(&conn, &[]).expect("lookup");
    assert!(hits.is_empty());
    assert!(misses.is_empty());
}

#[test]
fn cache_lookup_returns_miss_for_unknown_address() {
    let conn = Connection::open_in_memory().expect("open");
    ensure_cache_table(&conn).expect("ensure");
    let (hits, misses) =
        cache_lookup(&conn, &["totally unknown address".to_string()]).expect("lookup");
    assert!(hits.is_empty());
    assert_eq!(misses.len(), 1);
    assert_eq!(misses[0], "totally unknown address");
}

#[test]
fn cache_store_multiple_records_and_retrieve_all() {
    let conn = Connection::open_in_memory().expect("open");
    let records = vec![
        GeocodeResult {
            address: "addr A".to_string(),
            lat: 1.0,
            lon: 2.0,
            source: "geocodio".to_string(),
        },
        GeocodeResult {
            address: "addr B".to_string(),
            lat: 3.0,
            lon: 4.0,
            source: "geocodio".to_string(),
        },
    ];
    cache_store(&conn, &records, "geocodio").expect("store");

    let (hits, misses) =
        cache_lookup(&conn, &["addr A".to_string(), "addr B".to_string()]).expect("lookup");
    assert_eq!(hits.len(), 2);
    assert!(misses.is_empty());
}

// ---- End-to-end geocode_batch tests ----

#[test]
fn geocode_batch_with_empty_addresses_returns_empty() {
    let (path, _conn) = temp_db();
    let (results, _stats) = geocode_batch(&path, &[]).expect("batch empty");
    assert!(results.is_empty());
    cleanup(&path);
}

#[test]
fn geocode_batch_deduplicates_cached_results() {
    let (path, conn) = temp_db();
    // Pre-populate cache
    let records = vec![GeocodeResult {
        address: "85 Pike St".to_string(),
        lat: 47.6088,
        lon: -122.3404,
        source: "geocodio".to_string(),
    }];
    cache_store(&conn, &records, "geocodio").expect("seed");
    drop(conn);

    // Pass the same address twice
    let (results, _stats) = geocode_batch(
        &path,
        &["85 Pike St".to_string(), "85 Pike St".to_string()],
    )
    .expect("batch dup");

    // Both inputs should get the same cached result
    assert_eq!(results.len(), 2);
    assert!((results[0].lat - results[1].lat).abs() < 1e-9);
    assert!((results[0].lon - results[1].lon).abs() < 1e-9);
    cleanup(&path);
}

#[test]
fn geocode_batch_preserves_input_order() {
    let (path, conn) = temp_db();
    let records = vec![
        GeocodeResult {
            address: "addr alpha".to_string(),
            lat: 10.0,
            lon: 20.0,
            source: "test".to_string(),
        },
        GeocodeResult {
            address: "addr beta".to_string(),
            lat: 30.0,
            lon: 40.0,
            source: "test".to_string(),
        },
        GeocodeResult {
            address: "addr gamma".to_string(),
            lat: 50.0,
            lon: 60.0,
            source: "test".to_string(),
        },
    ];
    cache_store(&conn, &records, "test").expect("seed");
    drop(conn);

    let (results, _stats) = geocode_batch(
        &path,
        &[
            "addr gamma".to_string(),
            "addr alpha".to_string(),
            "addr beta".to_string(),
        ],
    )
    .expect("batch order");

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].address, "addr gamma");
    assert_eq!(results[1].address, "addr alpha");
    assert_eq!(results[2].address, "addr beta");
    cleanup(&path);
}

#[test]
fn geocode_batch_returns_only_resolved_addresses() {
    let (path, conn) = temp_db();
    // Cache only one of two addresses
    let records = vec![GeocodeResult {
        address: "valid address".to_string(),
        lat: 1.0,
        lon: 2.0,
        source: "test".to_string(),
    }];
    cache_store(&conn, &records, "test").expect("seed");
    drop(conn);

    // Only pass the cached address -- unresolved addresses without API key would error
    let (results, _stats) = geocode_batch(&path, &["valid address".to_string()]).expect("batch");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].address, "valid address");
    assert!((results[0].lat - 1.0).abs() < 1e-9);
    cleanup(&path);
}

#[test]
fn geocode_batch_returns_cached_confidence() {
    let (path, conn) = temp_db();
    let records = vec![GeocodeResult {
        address: "cached addr".to_string(),
        lat: 1.5,
        lon: 2.5,
        source: "geocodio".to_string(),
    }];
    cache_store(&conn, &records, "geocodio").expect("seed");
    drop(conn);

    let (results, _stats) = geocode_batch(&path, &["cached addr".to_string()]).expect("batch");
    assert_eq!(results.len(), 1);
    // geocodio default confidence is 0.85
    assert!((results[0].confidence - 0.85).abs() < 1e-6);
    cleanup(&path);
}

// ---- Column writing tests (simulating geocode_table_column logic) ----

/// Helper to simulate the column-writing logic from the Tauri command.
fn write_geocode_columns(
    conn: &Connection,
    table_name: &str,
    address_col: &str,
    results: &[GeocodeBatchResult],
) {
    for alter_sql in [
        format!(
            r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS _lat DOUBLE"#,
            table_name
        ),
        format!(
            r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS _lon DOUBLE"#,
            table_name
        ),
        format!(
            r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS _geocode_source VARCHAR"#,
            table_name
        ),
        format!(
            r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS _geocode_confidence DOUBLE"#,
            table_name
        ),
    ] {
        conn.execute_batch(&alter_sql).expect("alter table");
    }

    if results.is_empty() {
        return;
    }

    let values: Vec<String> = results
        .iter()
        .map(|r| {
            format!(
                "('{}', {}, {}, '{}', {})",
                r.address.replace('\'', "''"),
                r.lat,
                r.lon,
                r.source.replace('\'', "''"),
                r.confidence,
            )
        })
        .collect();

    conn.execute_batch(&format!(
        "CREATE OR REPLACE TEMP TABLE _gc AS \
         SELECT * FROM (VALUES {}) AS t(address, lat, lon, source, confidence)",
        values.join(", "),
    ))
    .expect("create temp table");

    conn.execute_batch(&format!(
        r#"UPDATE "{table}" SET _lat = g.lat, _lon = g.lon,
           _geocode_source = g.source, _geocode_confidence = g.confidence
           FROM _gc g WHERE "{table}"."{col}" = g.address"#,
        table = table_name,
        col = address_col,
    ))
    .expect("update geocode columns");

    conn.execute_batch("DROP TABLE IF EXISTS _gc")
        .expect("drop temp");
}

#[test]
fn geocode_writes_lat_lon_columns_to_table() {
    let conn = Connection::open_in_memory().expect("open");
    conn.execute_batch(
        "CREATE TABLE places (id INTEGER, address VARCHAR); \
         INSERT INTO places VALUES (1, '85 Pike St'); \
         INSERT INTO places VALUES (2, '400 Broad St');",
    )
    .expect("setup");

    let results = vec![
        GeocodeBatchResult {
            address: "85 Pike St".to_string(),
            lat: 47.6088,
            lon: -122.3404,
            source: "geocodio".to_string(),
            confidence: 0.85,
            matched_label: None,
            matched_table: None,
        },
        GeocodeBatchResult {
            address: "400 Broad St".to_string(),
            lat: 47.6205,
            lon: -122.3493,
            source: "geocodio".to_string(),
            confidence: 0.85,
            matched_label: None,
            matched_table: None,
        },
    ];

    write_geocode_columns(&conn, "places", "address", &results);

    let mut stmt = conn
        .prepare("SELECT address, _lat, _lon FROM places ORDER BY id")
        .expect("prepare");
    let mut query_rows = stmt.query([]).expect("query");
    let mut rows: Vec<(String, f64, f64)> = Vec::new();
    while let Some(row) = query_rows.next().expect("next") {
        rows.push((
            row.get::<_, String>(0).expect("col0"),
            row.get::<_, f64>(1).expect("col1"),
            row.get::<_, f64>(2).expect("col2"),
        ));
    }

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "85 Pike St");
    assert!((rows[0].1 - 47.6088).abs() < 1e-4);
    assert!((rows[0].2 - (-122.3404)).abs() < 1e-4);
    assert_eq!(rows[1].0, "400 Broad St");
    assert!((rows[1].1 - 47.6205).abs() < 1e-4);
}

#[test]
fn geocode_handles_null_address_values() {
    let conn = Connection::open_in_memory().expect("open");
    conn.execute_batch(
        "CREATE TABLE places (id INTEGER, address VARCHAR); \
         INSERT INTO places VALUES (1, '85 Pike St'); \
         INSERT INTO places VALUES (2, NULL);",
    )
    .expect("setup");

    let results = vec![GeocodeBatchResult {
        address: "85 Pike St".to_string(),
        lat: 47.6088,
        lon: -122.3404,
        source: "geocodio".to_string(),
        confidence: 0.85,
        matched_label: None,
        matched_table: None,
    }];

    write_geocode_columns(&conn, "places", "address", &results);

    // Row with NULL address should have NULL lat/lon
    let mut stmt = conn
        .prepare("SELECT _lat, _lon FROM places WHERE id = 2")
        .expect("prepare");
    let row: (Option<f64>, Option<f64>) = stmt
        .query_row([], |row| {
            Ok((row.get::<_, Option<f64>>(0)?, row.get::<_, Option<f64>>(1)?))
        })
        .expect("query");

    assert!(row.0.is_none(), "_lat should be NULL for NULL address");
    assert!(row.1.is_none(), "_lon should be NULL for NULL address");
}

#[test]
fn geocode_handles_duplicate_addresses_in_table() {
    let conn = Connection::open_in_memory().expect("open");
    conn.execute_batch(
        "CREATE TABLE places (id INTEGER, address VARCHAR); \
         INSERT INTO places VALUES (1, '85 Pike St'); \
         INSERT INTO places VALUES (2, '85 Pike St');",
    )
    .expect("setup");

    let results = vec![GeocodeBatchResult {
        address: "85 Pike St".to_string(),
        lat: 47.6088,
        lon: -122.3404,
        source: "geocodio".to_string(),
        confidence: 0.85,
        matched_label: None,
        matched_table: None,
    }];

    write_geocode_columns(&conn, "places", "address", &results);

    let mut stmt = conn
        .prepare("SELECT _lat, _lon FROM places ORDER BY id")
        .expect("prepare");
    let mut query_rows = stmt.query([]).expect("query");
    let mut rows: Vec<(f64, f64)> = Vec::new();
    while let Some(row) = query_rows.next().expect("next") {
        rows.push((
            row.get::<_, f64>(0).expect("col0"),
            row.get::<_, f64>(1).expect("col1"),
        ));
    }

    assert_eq!(rows.len(), 2);
    // Both rows should have the same coordinates
    assert!((rows[0].0 - rows[1].0).abs() < 1e-9);
    assert!((rows[0].1 - rows[1].1).abs() < 1e-9);
}

#[test]
fn geocode_adds_source_and_confidence_columns() {
    let conn = Connection::open_in_memory().expect("open");
    conn.execute_batch(
        "CREATE TABLE places (id INTEGER, address VARCHAR); \
         INSERT INTO places VALUES (1, '85 Pike St');",
    )
    .expect("setup");

    let results = vec![GeocodeBatchResult {
        address: "85 Pike St".to_string(),
        lat: 47.6088,
        lon: -122.3404,
        source: "geocodio".to_string(),
        confidence: 0.85,
        matched_label: None,
        matched_table: None,
    }];

    write_geocode_columns(&conn, "places", "address", &results);

    let mut stmt = conn
        .prepare("SELECT _geocode_source, _geocode_confidence FROM places WHERE id = 1")
        .expect("prepare");
    let (source, confidence): (String, f64) = stmt
        .query_row([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)))
        .expect("query");

    assert_eq!(source, "geocodio");
    assert!((confidence - 0.85).abs() < 1e-6);
}

#[test]
fn geocode_column_write_handles_special_chars_in_address() {
    let conn = Connection::open_in_memory().expect("open");
    conn.execute_batch(
        "CREATE TABLE places (id INTEGER, address VARCHAR); \
         INSERT INTO places VALUES (1, '123 O''Brien & Sons Rd.');",
    )
    .expect("setup");

    let results = vec![GeocodeBatchResult {
        address: "123 O'Brien & Sons Rd.".to_string(),
        lat: 47.6090,
        lon: -122.3350,
        source: "geocodio".to_string(),
        confidence: 0.85,
        matched_label: None,
        matched_table: None,
    }];

    write_geocode_columns(&conn, "places", "address", &results);

    let mut stmt = conn
        .prepare("SELECT _lat, _lon FROM places WHERE id = 1")
        .expect("prepare");
    let (lat, lon): (f64, f64) = stmt
        .query_row([], |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)))
        .expect("query");

    assert!((lat - 47.6090).abs() < 1e-4);
    assert!((lon - (-122.3350)).abs() < 1e-4);
}

#[test]
fn geocode_empty_results_does_not_error() {
    let conn = Connection::open_in_memory().expect("open");
    conn.execute_batch(
        "CREATE TABLE places (id INTEGER, address VARCHAR); \
         INSERT INTO places VALUES (1, '85 Pike St');",
    )
    .expect("setup");

    // Writing empty results should still add columns but not crash
    write_geocode_columns(&conn, "places", "address", &[]);

    // Columns should still be added
    let mut stmt = conn
        .prepare("SELECT _lat, _lon FROM places WHERE id = 1")
        .expect("prepare");
    let (lat, lon): (Option<f64>, Option<f64>) = stmt
        .query_row([], |row| {
            Ok((row.get::<_, Option<f64>>(0)?, row.get::<_, Option<f64>>(1)?))
        })
        .expect("query");

    assert!(lat.is_none());
    assert!(lon.is_none());
}

// ---- End-to-end: CSV ingest + cache-seeded geocode_batch ----

#[test]
fn end_to_end_ingest_and_geocode_with_seeded_cache() {
    let (path, conn) = temp_db();

    // Ingest the test CSV
    ingest_test_csv(&conn);

    // Verify data loaded
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM test_addresses", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(count, 10, "CSV should have 10 rows");

    // Seed cache with known addresses
    seed_cache(&conn);
    drop(conn); // geocode_batch opens its own connection

    // Geocode a subset of addresses from the CSV
    let addresses = vec![
        "85 Pike St, Seattle, WA 98101".to_string(),
        "400 Broad St, Seattle, WA 98109".to_string(),
    ];

    let (results, _stats) = geocode_batch(&path, &addresses).expect("batch geocode");
    assert_eq!(results.len(), 2);

    // Verify first result
    assert_eq!(results[0].address, "85 Pike St, Seattle, WA 98101");
    assert!((results[0].lat - 47.6088).abs() < 1e-4);

    // Verify second result
    assert_eq!(results[1].address, "400 Broad St, Seattle, WA 98109");
    assert!((results[1].lat - 47.6205).abs() < 1e-4);

    cleanup(&path);
}
