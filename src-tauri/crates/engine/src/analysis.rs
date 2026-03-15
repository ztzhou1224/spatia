use duckdb::Connection;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::sync::OnceLock;
use tracing::{debug, error, info};

use crate::EngineResult;

/// Raw tabular result limited to the first `TABULAR_ROW_LIMIT` rows.
/// Each inner `Vec<Value>` corresponds to one row; values are in column order.
#[derive(Debug, Clone, Serialize)]
pub struct TabularResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisExecutionResult {
    pub status: &'static str,
    pub row_count: usize,
    pub total_count: usize,
    pub geojson: Value,
    pub tabular: TabularResult,
}

/// Maximum rows included in the tabular preview.
const TABULAR_ROW_LIMIT: usize = 100;

/// Maximum features included in the GeoJSON result.
const GEOJSON_ROW_LIMIT: usize = 5000;

/// Drop all `_spatia_step_*` intermediate views from the given connection.
/// Errors are logged but not propagated, since this is a best-effort cleanup.
fn cleanup_intermediate_views(conn: &Connection) {
    for n in 1..=MAX_INTERMEDIATE_STEPS {
        let name = step_view_name(n);
        let drop_sql = format!("DROP VIEW IF EXISTS {name}");
        if let Err(e) = conn.execute_batch(&drop_sql) {
            error!(view = %name, error = %e, "execute_analysis_sql: failed to drop intermediate view during cleanup");
        }
    }
}

pub fn execute_analysis_sql_to_geojson(
    db_path: &str,
    sql: &str,
) -> EngineResult<AnalysisExecutionResult> {
    info!("execute_analysis_sql: starting analysis SQL execution");
    debug!(sql = %sql, "execute_analysis_sql: SQL statement");

    validate_analysis_sql(sql)?;

    // Split into individual statements (same logic as validate_analysis_sql).
    let statements: Vec<&str> = sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let conn = Connection::open(db_path)?;

    // Execute each statement in order. On failure, clean up intermediate views
    // before returning the error.
    for (i, stmt) in statements.iter().enumerate() {
        if let Err(e) = conn.execute_batch(stmt) {
            let step_label = if i + 1 < statements.len() {
                format!("Step {}", i + 1)
            } else {
                "Final step".to_string()
            };
            error!(
                step = step_label,
                sql = %stmt,
                error = %e,
                "execute_analysis_sql: failed to execute statement"
            );
            cleanup_intermediate_views(&conn);
            return Err(format!("{step_label} failed: {e}").into());
        }
    }

    // Read results into an owned value, then always clean up intermediate views
    // regardless of whether reading succeeds or fails.
    let read_result = read_analysis_result(&conn);
    cleanup_intermediate_views(&conn);
    read_result
}

/// Read from the `analysis_result` view and build the `AnalysisExecutionResult`.
fn read_analysis_result(conn: &Connection) -> EngineResult<AnalysisExecutionResult> {
    let mut schema_stmt = conn.prepare(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'main' AND table_name = 'analysis_result' \
         ORDER BY ordinal_position"
    )?;
    let mut schema_rows = schema_stmt.query([])?;
    let mut col_names: Vec<String> = Vec::new();
    while let Some(row) = schema_rows.next()? {
        col_names.push(row.get::<_, String>(0)?);
    }

    // Build a CAST-to-VARCHAR select list so that non-string column types
    // (BIGINT, DOUBLE, DATE, etc.) are returned as strings. The duckdb-rs
    // driver returns Err for `row.get::<_, String>(i)` on non-VARCHAR columns;
    // `.ok()` converts that to None → Value::Null, making numeric columns
    // appear as null in results. CAST avoids that silently-null bug.
    let cast_select = col_names
        .iter()
        .map(|c| format!(r#"CAST("{c}" AS VARCHAR) AS "{c}""#))
        .collect::<Vec<_>>()
        .join(", ");

    // --- Total count (before truncation) ---
    let total_count: usize = {
        let mut count_stmt = conn.prepare("SELECT COUNT(*) FROM analysis_result")?;
        let mut count_rows = count_stmt.query([])?;
        match count_rows.next()? {
            Some(row) => row.get::<_, i64>(0).unwrap_or(0) as usize,
            None => 0,
        }
    };

    // --- GeoJSON pass (up to GEOJSON_ROW_LIMIT rows) ---
    let mut stmt = conn.prepare(&format!(
        "SELECT {cast_select} FROM analysis_result LIMIT {GEOJSON_ROW_LIMIT}"
    ))?;

    let mut rows = stmt.query([])?;
    let mut features: Vec<Value> = Vec::new();

    while let Some(row) = rows.next()? {
        let mut props = Map::new();

        for (index, column_name) in col_names.iter().enumerate() {
            let cell: Option<String> = row.get(index).ok();
            match cell {
                Some(value) => {
                    props.insert(column_name.clone(), Value::String(value));
                }
                None => {
                    props.insert(column_name.clone(), Value::Null);
                }
            }
        }

        let lat = parse_number_property(&props, &["lat", "latitude", "_lat"]);
        let lon = parse_number_property(&props, &["lon", "lng", "longitude", "_lon"]);

        let geometry = match (lat, lon) {
            (Some(lat), Some(lon)) => {
                json!({ "type": "Point", "coordinates": [lon, lat] })
            }
            _ => Value::Null,
        };

        features.push(json!({
            "type": "Feature",
            "geometry": geometry,
            "properties": Value::Object(props),
        }));
    }

    // --- Tabular pass (up to TABULAR_ROW_LIMIT + 1 to detect truncation) ---
    let fetch_limit = TABULAR_ROW_LIMIT + 1;
    let mut tab_stmt = conn.prepare(&format!(
        "SELECT {cast_select} FROM analysis_result LIMIT {fetch_limit}"
    ))?;
    let mut tab_rows = tab_stmt.query([])?;
    let mut raw_rows: Vec<Vec<Value>> = Vec::new();

    while let Some(row) = tab_rows.next()? {
        let mut cells: Vec<Value> = Vec::with_capacity(col_names.len());
        for index in 0..col_names.len() {
            let cell: Option<String> = row.get(index).ok();
            cells.push(match cell {
                Some(v) => Value::String(v),
                None => Value::Null,
            });
        }
        raw_rows.push(cells);
    }

    let truncated = raw_rows.len() > TABULAR_ROW_LIMIT;
    raw_rows.truncate(TABULAR_ROW_LIMIT);

    let tabular = TabularResult {
        columns: col_names,
        rows: raw_rows,
        truncated,
    };

    info!(row_count = features.len(), total_count = total_count, "execute_analysis_sql: completed successfully");
    Ok(AnalysisExecutionResult {
        status: "ok",
        row_count: features.len(),
        total_count,
        geojson: json!({
            "type": "FeatureCollection",
            "features": features,
        }),
        tabular,
    })
}

fn parse_number_property(props: &Map<String, Value>, names: &[&str]) -> Option<f64> {
    for (key, value) in props {
        if !names.iter().any(|name| key.eq_ignore_ascii_case(name)) {
            continue;
        }

        match value {
            Value::String(text) => {
                if let Ok(parsed) = text.parse::<f64>() {
                    return Some(parsed);
                }
            }
            Value::Number(number) => {
                if let Some(parsed) = number.as_f64() {
                    return Some(parsed);
                }
            }
            _ => {}
        }
    }
    None
}

/// Dangerous SQL keyword patterns that must not appear anywhere in analysis SQL.
/// Each entry is `(display_name, regex_pattern)`. Patterns use `\b` word boundaries
/// so that identifiers like `drop_count` or `update_time` are not flagged.
static BLOCKLIST: &[(&str, &str)] = &[
    ("DROP TABLE",    r"(?i)\bDROP\s+TABLE\b"),
    ("DROP VIEW",     r"(?i)\bDROP\s+VIEW\b"),
    ("DROP SCHEMA",   r"(?i)\bDROP\s+SCHEMA\b"),
    ("DROP DATABASE", r"(?i)\bDROP\s+DATABASE\b"),
    ("TRUNCATE",      r"(?i)\bTRUNCATE\b"),
    ("DELETE FROM",   r"(?i)\bDELETE\s+FROM\b"),
    ("ALTER TABLE",   r"(?i)\bALTER\s+TABLE\b"),
    ("ALTER VIEW",    r"(?i)\bALTER\s+VIEW\b"),
    ("GRANT",         r"(?i)\bGRANT\b"),
    ("REVOKE",        r"(?i)\bREVOKE\b"),
    ("INSERT INTO",   r"(?i)\bINSERT\s+INTO\b"),
    ("UPDATE",        r"(?i)\bUPDATE\b"),
    ("COPY",          r"(?i)\bCOPY\b"),
    ("ATTACH",        r"(?i)\bATTACH\b"),
    ("DETACH",        r"(?i)\bDETACH\b"),
];

/// Compiled regex cache — built once per process.
fn blocklist_regexes() -> &'static Vec<(&'static str, Regex)> {
    static CACHE: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    CACHE.get_or_init(|| {
        BLOCKLIST
            .iter()
            .map(|(name, pattern)| (*name, Regex::new(pattern).expect("valid blocklist pattern")))
            .collect()
    })
}

/// Strip the CREATE [OR REPLACE] VIEW <name> AS prefix from a statement (uppercase-normalized).
/// Returns the body after the prefix, or None if the prefix is not present.
fn strip_view_prefix<'a>(normalized: &'a str, view_name: &str) -> Option<&'a str> {
    let prefix_or = format!("CREATE OR REPLACE VIEW {} AS", view_name.to_uppercase());
    let prefix_plain = format!("CREATE VIEW {} AS", view_name.to_uppercase());
    if normalized.starts_with(&prefix_or) {
        Some(&normalized[prefix_or.len()..])
    } else if normalized.starts_with(&prefix_plain) {
        Some(&normalized[prefix_plain.len()..])
    } else {
        None
    }
}

/// Returns the intermediate view name for step N (1-indexed), e.g. "_spatia_step_1".
fn step_view_name(n: usize) -> String {
    format!("_spatia_step_{n}")
}

/// Maximum number of intermediate `_spatia_step_*` views permitted.
const MAX_INTERMEDIATE_STEPS: usize = 5;

fn validate_analysis_sql(sql: &str) -> EngineResult<()> {
    // Split into individual statements, discarding empty ones produced by
    // trailing semicolons or whitespace-only segments.
    let statements: Vec<&str> = sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if statements.is_empty() {
        return Err("analysis SQL must not be empty".into());
    }

    // Cap: at most MAX_INTERMEDIATE_STEPS intermediate views + 1 final view.
    let max_total = MAX_INTERMEDIATE_STEPS + 1;
    if statements.len() > max_total {
        return Err(format!(
            "analysis SQL may contain at most {MAX_INTERMEDIATE_STEPS} intermediate steps \
             plus one final analysis_result view ({max_total} statements total); \
             got {} statements",
            statements.len()
        )
        .into());
    }

    // Run the blocklist scan on the entire SQL first. This ensures that any
    // dangerous keyword anywhere in the input (including extra statements after
    // the final view) is caught with an informative error before we do
    // structural validation.
    //
    // We scan the full raw SQL rather than individual statement bodies so that
    // the word-boundary regexes work correctly across statement boundaries.
    for (name, re) in blocklist_regexes() {
        if re.is_match(sql) {
            return Err(format!(
                "analysis SQL contains a disallowed statement: {name}. \
                 Only read-only SELECT queries are permitted in the view body."
            )
            .into());
        }
    }

    // Structural validation: all statements except the last must be
    // `CREATE [OR REPLACE] VIEW _spatia_step_N AS ...` (N = 1..=5),
    // and the last must be `CREATE [OR REPLACE] VIEW analysis_result AS ...`.
    let last_idx = statements.len() - 1;
    for (i, stmt) in statements.iter().enumerate() {
        let normalized = stmt.to_uppercase();
        if i < last_idx {
            // Intermediate step: must be _spatia_step_<i+1>
            let expected_name = step_view_name(i + 1);
            if strip_view_prefix(&normalized, &expected_name).is_none() {
                return Err(format!(
                    "intermediate statement {} must be \
                     CREATE [OR REPLACE] VIEW {expected_name} AS ...; \
                     got: {stmt}",
                    i + 1,
                )
                .into());
            }
        } else {
            // Final statement must create analysis_result
            if strip_view_prefix(&normalized, "analysis_result").is_none() {
                return Err(
                    "analysis SQL must end with CREATE [OR REPLACE] VIEW analysis_result AS ..."
                        .into(),
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{execute_analysis_sql_to_geojson, validate_analysis_sql};
    use duckdb::Connection;
    use serde_json::Value;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn temp_db_path() -> String {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        format!("/tmp/spatia_analysis_test_{suffix}.duckdb")
    }

    fn cleanup_temp_db(db_path: &str) {
        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(format!("{db_path}.wal"));
        let _ = fs::remove_file(format!("{db_path}.wal.lck"));
    }

    // -----------------------------------------------------------------------
    // Execution integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn executes_view_sql_and_returns_geojson_points() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            "CREATE TABLE points(city VARCHAR, lat DOUBLE, lon DOUBLE)",
            [],
        )
        .expect("create table");
        conn.execute(
            "INSERT INTO points VALUES ('Seattle', 47.6062, -122.3321)",
            [],
        )
        .expect("insert row 1");
        conn.execute(
            "INSERT INTO points VALUES ('Portland', 45.5152, -122.6784)",
            [],
        )
        .expect("insert row 2");

        let sql = "CREATE OR REPLACE VIEW analysis_result AS SELECT city, lat, lon FROM points";
        let result = execute_analysis_sql_to_geojson(&db_path, sql).expect("execute analysis sql");

        assert_eq!(result.status, "ok");
        assert_eq!(result.row_count, 2);

        let features = result
            .geojson
            .get("features")
            .and_then(Value::as_array)
            .expect("features array");
        assert_eq!(features.len(), 2);

        cleanup_temp_db(&db_path);
    }

    #[test]
    fn rejects_non_view_sql() {
        let db_path = temp_db_path();
        let err = execute_analysis_sql_to_geojson(&db_path, "SELECT 1")
            .expect_err("expected validation error");
        assert!(err
            .to_string()
            .contains("CREATE [OR REPLACE] VIEW analysis_result AS"));
        cleanup_temp_db(&db_path);
    }

    // -----------------------------------------------------------------------
    // validate_analysis_sql — prefix check
    // -----------------------------------------------------------------------

    #[test]
    fn accepts_create_or_replace_view_prefix() {
        assert!(validate_analysis_sql(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1"
        )
        .is_ok());
    }

    #[test]
    fn accepts_create_view_prefix() {
        assert!(
            validate_analysis_sql("CREATE VIEW analysis_result AS SELECT 1").is_ok()
        );
    }

    #[test]
    fn rejects_missing_prefix() {
        let err = validate_analysis_sql("SELECT 1").expect_err("should reject");
        assert!(err.to_string().contains("CREATE [OR REPLACE] VIEW analysis_result AS"));
    }

    // -----------------------------------------------------------------------
    // validate_analysis_sql — allowed patterns (no false positives)
    // -----------------------------------------------------------------------

    #[test]
    fn allows_select_with_joins_and_aggregates() {
        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   SELECT a.city, COUNT(*) AS cnt, AVG(a.lat) AS avg_lat \
                   FROM addresses a \
                   JOIN regions r ON a.region_id = r.id \
                   WHERE a.active = true \
                   GROUP BY a.city \
                   HAVING COUNT(*) > 1 \
                   ORDER BY cnt DESC";
        assert!(validate_analysis_sql(sql).is_ok());
    }

    #[test]
    fn allows_cte_with_window_function() {
        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   WITH ranked AS ( \
                       SELECT name, score, RANK() OVER (ORDER BY score DESC) AS rnk \
                       FROM results \
                   ) \
                   SELECT * FROM ranked WHERE rnk <= 10";
        assert!(validate_analysis_sql(sql).is_ok());
    }

    /// Column names that *contain* blocked keywords as substrings must not be flagged.
    #[test]
    fn allows_column_names_containing_blocked_words() {
        // drop_count, update_time, truncation_flag, delete_marker,
        // grant_amount, insert_date, copy_number, attached_id
        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   SELECT drop_count, update_time, truncation_flag, \
                          delete_marker, grant_amount, insert_date, \
                          copy_number, attached_id, revoke_flag, detach_reason \
                   FROM events";
        assert!(validate_analysis_sql(sql).is_ok());
    }

    /// Table aliases that contain blocked substrings must not be flagged.
    #[test]
    fn allows_table_alias_containing_blocked_words() {
        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   SELECT updates.id, copies.name \
                   FROM changelog AS updates \
                   JOIN media AS copies ON updates.media_id = copies.id";
        assert!(validate_analysis_sql(sql).is_ok());
    }

    // -----------------------------------------------------------------------
    // validate_analysis_sql — blocked patterns
    // -----------------------------------------------------------------------

    fn assert_blocked(sql: &str, pattern_name: &str) {
        let err = validate_analysis_sql(sql).expect_err(&format!(
            "expected '{pattern_name}' to be blocked in: {sql}"
        ));
        let msg = err.to_string();
        assert!(
            msg.contains(pattern_name),
            "error message '{msg}' should mention '{pattern_name}'"
        );
    }

    #[test]
    fn blocks_drop_table() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; DROP TABLE sensitive",
            "DROP TABLE",
        );
    }

    #[test]
    fn blocks_drop_view() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; DROP VIEW old_view",
            "DROP VIEW",
        );
    }

    #[test]
    fn blocks_drop_schema() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; DROP SCHEMA public",
            "DROP SCHEMA",
        );
    }

    #[test]
    fn blocks_drop_database() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; DROP DATABASE prod",
            "DROP DATABASE",
        );
    }

    #[test]
    fn blocks_truncate() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; TRUNCATE logs",
            "TRUNCATE",
        );
    }

    #[test]
    fn blocks_delete_from() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; DELETE FROM users",
            "DELETE FROM",
        );
    }

    #[test]
    fn blocks_alter_table() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; ALTER TABLE t ADD COLUMN x INT",
            "ALTER TABLE",
        );
    }

    #[test]
    fn blocks_alter_view() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; ALTER VIEW v AS SELECT 2",
            "ALTER VIEW",
        );
    }

    #[test]
    fn blocks_grant() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; GRANT SELECT ON t TO user1",
            "GRANT",
        );
    }

    #[test]
    fn blocks_revoke() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; REVOKE SELECT ON t FROM user1",
            "REVOKE",
        );
    }

    #[test]
    fn blocks_insert_into() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; INSERT INTO t VALUES (1)",
            "INSERT INTO",
        );
    }

    #[test]
    fn blocks_update() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; UPDATE t SET x = 1",
            "UPDATE",
        );
    }

    #[test]
    fn blocks_copy() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; COPY t TO '/tmp/out.csv'",
            "COPY",
        );
    }

    #[test]
    fn blocks_attach() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; ATTACH '/other.db' AS other",
            "ATTACH",
        );
    }

    #[test]
    fn blocks_detach() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; DETACH other",
            "DETACH",
        );
    }

    /// Blocked keywords in mixed/lower case must still be caught.
    #[test]
    fn blocks_keywords_case_insensitively() {
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; drop table t",
            "DROP TABLE",
        );
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; Truncate logs",
            "TRUNCATE",
        );
        assert_blocked(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1; update t set x=1",
            "UPDATE",
        );
    }

    /// A blocked keyword embedded inside a subquery must still be caught.
    #[test]
    fn blocks_dangerous_subquery() {
        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   SELECT * FROM (SELECT 1) sub; DELETE FROM users WHERE 1=1";
        assert_blocked(sql, "DELETE FROM");
    }

    // -----------------------------------------------------------------------
    // Execution integration tests — new coverage (TASK-11)
    // -----------------------------------------------------------------------

    /// TC-011-01: Aggregation query returns correct tabular results.
    /// The view groups rows by category; tabular output must reflect each group
    /// with the right count, and the GeoJSON must be an empty FeatureCollection
    /// (no lat/lon columns in the result set).
    #[test]
    fn aggregation_query_returns_correct_tabular_results() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE sales(category VARCHAR, amount DOUBLE); \
             INSERT INTO sales VALUES ('A', 10.0); \
             INSERT INTO sales VALUES ('A', 20.0); \
             INSERT INTO sales VALUES ('B', 5.0);",
        )
        .expect("setup table");
        drop(conn);

        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   SELECT category, COUNT(*) AS cnt \
                   FROM sales \
                   GROUP BY category \
                   ORDER BY category";
        let result = execute_analysis_sql_to_geojson(&db_path, sql).expect("execute");

        // Tabular: 2 groups — A(2), B(1)
        assert_eq!(result.tabular.columns, vec!["category", "cnt"]);
        assert_eq!(result.tabular.rows.len(), 2);
        assert_eq!(result.tabular.rows[0][0], Value::String("A".to_string()));
        assert_eq!(result.tabular.rows[0][1], Value::String("2".to_string()));
        assert_eq!(result.tabular.rows[1][0], Value::String("B".to_string()));
        assert_eq!(result.tabular.rows[1][1], Value::String("1".to_string()));
        assert!(!result.tabular.truncated);

        // No lat/lon columns → row_count is still 2 (features are present but
        // geometry is null; the function counts all rows it iterates)
        let features = result
            .geojson
            .get("features")
            .and_then(Value::as_array)
            .expect("features array");
        assert_eq!(features.len(), 2);
        // Each feature's geometry must be null because there are no coordinate columns
        for f in features {
            assert_eq!(f.get("geometry").unwrap(), &Value::Null);
        }

        cleanup_temp_db(&db_path);
    }

    /// TC-011-02: Spatial query with _lat/_lon columns produces Point geometry.
    /// Properties must be included on every GeoJSON feature.
    #[test]
    fn spatial_query_with_underscore_lat_lon_produces_point_geometry() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE locations(name VARCHAR, score INTEGER, _lat DOUBLE, _lon DOUBLE); \
             INSERT INTO locations VALUES ('Alpha', 1, 47.6062, -122.3321); \
             INSERT INTO locations VALUES ('Beta',  2, 45.5152, -122.6784);",
        )
        .expect("setup table");
        drop(conn);

        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   SELECT name, score, _lat, _lon FROM locations WHERE score >= 1";
        let result = execute_analysis_sql_to_geojson(&db_path, sql).expect("execute");

        assert_eq!(result.status, "ok");
        assert_eq!(result.row_count, 2);

        let features = result
            .geojson
            .get("features")
            .and_then(Value::as_array)
            .expect("features array");
        assert_eq!(features.len(), 2);

        // Verify the first feature has a Point geometry with [lon, lat] ordering
        let geom = features[0].get("geometry").expect("geometry key");
        assert_eq!(geom.get("type").and_then(Value::as_str), Some("Point"));
        let coords = geom
            .get("coordinates")
            .and_then(Value::as_array)
            .expect("coordinates");
        assert_eq!(coords.len(), 2);
        // coordinates are [lon, lat]
        let lon = coords[0].as_f64().expect("lon f64");
        let lat = coords[1].as_f64().expect("lat f64");
        assert!((lon - (-122.3321)).abs() < 1e-4, "unexpected lon {lon}");
        assert!((lat - 47.6062).abs() < 1e-4, "unexpected lat {lat}");

        // Verify properties are present on the feature
        let props = features[0]
            .get("properties")
            .and_then(Value::as_object)
            .expect("properties object");
        assert!(props.contains_key("name"), "missing 'name' property");
        assert!(props.contains_key("score"), "missing 'score' property");
        assert_eq!(
            props.get("name").unwrap(),
            &Value::String("Alpha".to_string())
        );

        cleanup_temp_db(&db_path);
    }

    /// TC-011-03: Query without lat/lon columns returns empty GeoJSON geometry
    /// but tabular results still contain data.
    #[test]
    fn non_spatial_query_returns_empty_geometry_but_tabular_has_data() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE metrics(region VARCHAR, value INTEGER); \
             INSERT INTO metrics VALUES ('North', 42); \
             INSERT INTO metrics VALUES ('South', 17);",
        )
        .expect("setup table");
        drop(conn);

        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   SELECT region, value FROM metrics";
        let result = execute_analysis_sql_to_geojson(&db_path, sql).expect("execute");

        // All features have null geometry
        let features = result
            .geojson
            .get("features")
            .and_then(Value::as_array)
            .expect("features array");
        assert_eq!(features.len(), 2);
        for f in features {
            assert_eq!(
                f.get("geometry").unwrap(),
                &Value::Null,
                "geometry should be null when no lat/lon present"
            );
        }

        // Tabular should still have both rows
        assert_eq!(result.tabular.columns, vec!["region", "value"]);
        assert_eq!(result.tabular.rows.len(), 2);

        cleanup_temp_db(&db_path);
    }

    /// TC-011-04: A WHERE clause that matches nothing produces an empty result —
    /// zero GeoJSON features and zero tabular rows, no error.
    #[test]
    fn empty_result_set_returns_zero_features_and_zero_tabular_rows() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE things(id INTEGER, active BOOLEAN); \
             INSERT INTO things VALUES (1, false); \
             INSERT INTO things VALUES (2, false);",
        )
        .expect("setup table");
        drop(conn);

        // The WHERE clause matches no rows
        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   SELECT id, active FROM things WHERE active = true";
        let result = execute_analysis_sql_to_geojson(&db_path, sql).expect("execute — should not error on empty set");

        assert_eq!(result.status, "ok");
        assert_eq!(result.row_count, 0);

        let features = result
            .geojson
            .get("features")
            .and_then(Value::as_array)
            .expect("features array");
        assert!(features.is_empty(), "expected no features for empty result");

        assert!(
            result.tabular.rows.is_empty(),
            "expected no tabular rows for empty result"
        );
        assert!(!result.tabular.truncated);

        cleanup_temp_db(&db_path);
    }

    /// TC-011-05: When the result set exceeds 20 rows, tabular output is truncated
    /// to exactly 20 rows and `truncated` is set to true.
    /// GeoJSON fetches up to 1000 rows so it should contain all 30.
    #[test]
    fn tabular_result_is_truncated_at_twenty_rows() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        // Insert 30 rows
        conn.execute_batch("CREATE TABLE nums(n INTEGER)")
            .expect("create table");
        for i in 0..30_i32 {
            conn.execute("INSERT INTO nums VALUES (?)", [i])
                .expect("insert");
        }
        drop(conn);

        let sql = "CREATE OR REPLACE VIEW analysis_result AS SELECT n FROM nums";
        let result = execute_analysis_sql_to_geojson(&db_path, sql).expect("execute");

        // Tabular is capped at 20
        assert_eq!(
            result.tabular.rows.len(),
            20,
            "tabular rows should be truncated to 20"
        );
        assert!(result.tabular.truncated, "truncated flag must be true");

        // GeoJSON side fetches up to 1000 — all 30 should be present
        assert_eq!(result.row_count, 30);

        cleanup_temp_db(&db_path);
    }

    /// TC-011-06: Mixed column types (VARCHAR, INTEGER, DOUBLE, DATE, BOOLEAN)
    /// all appear as strings in tabular results (the CAST-to-VARCHAR path).
    #[test]
    fn mixed_column_types_appear_as_strings_in_tabular_results() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE typed(\
               label VARCHAR, \
               count INTEGER, \
               score DOUBLE, \
               created DATE, \
               active BOOLEAN \
             ); \
             INSERT INTO typed VALUES ('hello', 42, 3.14, DATE '2024-06-01', true);",
        )
        .expect("setup table");
        drop(conn);

        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   SELECT label, count, score, created, active FROM typed";
        let result = execute_analysis_sql_to_geojson(&db_path, sql).expect("execute");

        assert_eq!(result.tabular.rows.len(), 1);
        let row = &result.tabular.rows[0];

        // All values arrive as JSON strings via the CAST-to-VARCHAR logic
        assert!(
            matches!(&row[0], Value::String(s) if s == "hello"),
            "label should be string 'hello', got {:?}", row[0]
        );
        assert!(
            matches!(&row[1], Value::String(s) if s == "42"),
            "count should be string '42', got {:?}", row[1]
        );
        // DOUBLE representation can vary slightly; just confirm it's a non-null string
        assert!(
            matches!(&row[2], Value::String(s) if s.starts_with("3.1")),
            "score should be a string starting with '3.1', got {:?}", row[2]
        );
        // DATE should appear as ISO-8601 string
        assert!(
            matches!(&row[3], Value::String(s) if s.contains("2024-06-01")),
            "created should contain '2024-06-01', got {:?}", row[3]
        );
        // BOOLEAN — DuckDB CAST(true AS VARCHAR) produces "true"
        assert!(
            matches!(&row[4], Value::String(s) if s.eq_ignore_ascii_case("true")),
            "active should be string 'true', got {:?}", row[4]
        );

        cleanup_temp_db(&db_path);
    }

    /// TC-011-07: Column names returned in `tabular.columns` preserve the SELECT
    /// order and match exactly what the view exposes.
    #[test]
    fn tabular_columns_match_view_select_order() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE t(z INTEGER, a INTEGER, m INTEGER); \
             INSERT INTO t VALUES (3, 1, 2);",
        )
        .expect("setup table");
        drop(conn);

        // Deliberately select in non-alphabetical order
        let sql = "CREATE OR REPLACE VIEW analysis_result AS SELECT z, a, m FROM t";
        let result = execute_analysis_sql_to_geojson(&db_path, sql).expect("execute");

        assert_eq!(result.tabular.columns, vec!["z", "a", "m"]);
        let row = &result.tabular.rows[0];
        assert_eq!(row[0], Value::String("3".to_string()), "z");
        assert_eq!(row[1], Value::String("1".to_string()), "a");
        assert_eq!(row[2], Value::String("2".to_string()), "m");

        cleanup_temp_db(&db_path);
    }

    /// TC-011-08: Complex SELECT with CTE and subquery executes end-to-end
    /// and returns correct features and tabular data.
    #[test]
    fn cte_and_subquery_execute_end_to_end() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE readings(sensor VARCHAR, val DOUBLE, lat DOUBLE, lon DOUBLE); \
             INSERT INTO readings VALUES ('A', 9.0, 47.6, -122.3); \
             INSERT INTO readings VALUES ('A', 7.0, 47.6, -122.3); \
             INSERT INTO readings VALUES ('B', 4.0, 45.5, -122.7);",
        )
        .expect("setup table");
        drop(conn);

        let sql = "CREATE OR REPLACE VIEW analysis_result AS \
                   WITH avg_vals AS ( \
                       SELECT sensor, AVG(val) AS avg_val, \
                              AVG(lat) AS lat, AVG(lon) AS lon \
                       FROM readings \
                       GROUP BY sensor \
                   ) \
                   SELECT * FROM avg_vals WHERE avg_val > 5.0 \
                   ORDER BY sensor";
        let result = execute_analysis_sql_to_geojson(&db_path, sql).expect("execute");

        // Only sensor 'A' (avg=8.0) qualifies; 'B' (avg=4.0) does not
        assert_eq!(result.row_count, 1);
        assert_eq!(result.tabular.rows.len(), 1);

        let row = &result.tabular.rows[0];
        assert_eq!(row[0], Value::String("A".to_string()), "sensor column");

        // The CTE averages lat/lon, so the feature should have a Point geometry
        let features = result
            .geojson
            .get("features")
            .and_then(Value::as_array)
            .expect("features");
        let geom = features[0].get("geometry").expect("geometry key");
        assert_eq!(
            geom.get("type").and_then(Value::as_str),
            Some("Point"),
            "CTE result with lat/lon should produce a Point geometry"
        );

        cleanup_temp_db(&db_path);
    }

    // -----------------------------------------------------------------------
    // Multi-step analysis SQL — validation tests
    // -----------------------------------------------------------------------

    #[test]
    fn validates_valid_multi_step_sql() {
        let sql = "CREATE OR REPLACE VIEW _spatia_step_1 AS SELECT id, val FROM raw; \
                   CREATE OR REPLACE VIEW _spatia_step_2 AS SELECT id, val * 2 AS val2 FROM _spatia_step_1; \
                   CREATE OR REPLACE VIEW analysis_result AS SELECT id, val2 FROM _spatia_step_2";
        assert!(validate_analysis_sql(sql).is_ok(), "valid multi-step SQL should pass validation");
    }

    #[test]
    fn validates_single_step_backward_compatible() {
        // Single-statement SQL must still work (backward compatibility).
        assert!(validate_analysis_sql(
            "CREATE OR REPLACE VIEW analysis_result AS SELECT 1 AS n"
        ).is_ok());
        assert!(validate_analysis_sql(
            "CREATE VIEW analysis_result AS SELECT 1 AS n"
        ).is_ok());
    }

    #[test]
    fn rejects_too_many_intermediate_steps() {
        // 6 intermediate steps + 1 final = 7 total, exceeds max of 6.
        let sql = "CREATE OR REPLACE VIEW _spatia_step_1 AS SELECT 1; \
                   CREATE OR REPLACE VIEW _spatia_step_2 AS SELECT 1; \
                   CREATE OR REPLACE VIEW _spatia_step_3 AS SELECT 1; \
                   CREATE OR REPLACE VIEW _spatia_step_4 AS SELECT 1; \
                   CREATE OR REPLACE VIEW _spatia_step_5 AS SELECT 1; \
                   CREATE OR REPLACE VIEW _spatia_step_6 AS SELECT 1; \
                   CREATE OR REPLACE VIEW analysis_result AS SELECT 1";
        let err = validate_analysis_sql(sql).expect_err("should reject excessive steps");
        assert!(
            err.to_string().contains("at most 5 intermediate steps"),
            "error should mention step limit: {err}"
        );
    }

    #[test]
    fn rejects_wrong_intermediate_view_name() {
        // Step 1 view has wrong name (should be _spatia_step_1, not _step_1).
        let sql = "CREATE OR REPLACE VIEW _step_1 AS SELECT 1; \
                   CREATE OR REPLACE VIEW analysis_result AS SELECT 1";
        let err = validate_analysis_sql(sql).expect_err("should reject wrong intermediate view name");
        assert!(
            err.to_string().contains("_spatia_step_1"),
            "error should mention expected name: {err}"
        );
    }

    #[test]
    fn rejects_out_of_order_intermediate_step() {
        // Steps must be numbered sequentially: step 1, then step 2, etc.
        // Providing step 2 as the first intermediate should fail.
        let sql = "CREATE OR REPLACE VIEW _spatia_step_2 AS SELECT 1; \
                   CREATE OR REPLACE VIEW analysis_result AS SELECT 1";
        let err = validate_analysis_sql(sql).expect_err("should reject out-of-order step");
        assert!(
            err.to_string().contains("_spatia_step_1"),
            "error should mention expected name _spatia_step_1: {err}"
        );
    }

    #[test]
    fn rejects_missing_final_analysis_result() {
        // Multi-step SQL that ends with an intermediate view instead of analysis_result.
        let sql = "CREATE OR REPLACE VIEW _spatia_step_1 AS SELECT 1; \
                   CREATE OR REPLACE VIEW _spatia_step_2 AS SELECT 1";
        let err = validate_analysis_sql(sql).expect_err("should reject missing analysis_result");
        assert!(
            err.to_string().contains("analysis_result"),
            "error should mention analysis_result: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // Multi-step analysis SQL — execution integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn multi_step_execution_produces_correct_result() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE sales(region VARCHAR, amount DOUBLE); \
             INSERT INTO sales VALUES ('East', 100.0); \
             INSERT INTO sales VALUES ('East', 200.0); \
             INSERT INTO sales VALUES ('West', 50.0);",
        )
        .expect("setup table");
        drop(conn);

        // Step 1: sum by region. Step 2 (final): select only regions above threshold.
        let sql = "CREATE OR REPLACE VIEW _spatia_step_1 AS \
                       SELECT region, SUM(amount) AS total FROM sales GROUP BY region; \
                   CREATE OR REPLACE VIEW analysis_result AS \
                       SELECT region, total FROM _spatia_step_1 WHERE total > 100 ORDER BY region";

        let result = execute_analysis_sql_to_geojson(&db_path, sql).expect("execute multi-step");
        assert_eq!(result.status, "ok");
        // Only East (total=300) qualifies; West (total=50) does not.
        assert_eq!(result.tabular.rows.len(), 1);
        assert_eq!(
            result.tabular.rows[0][0],
            Value::String("East".to_string()),
            "region column"
        );
        assert_eq!(
            result.tabular.rows[0][1],
            Value::String("300.0".to_string()),
            "total column"
        );

        // Verify intermediate views are cleaned up after execution.
        let check_conn = Connection::open(&db_path).expect("open db for check");
        let view_exists: i32 = check_conn
            .query_row(
                "SELECT COUNT(*) FROM information_schema.tables \
                 WHERE table_schema = 'main' AND table_name = '_spatia_step_1'",
                [],
                |row| row.get(0),
            )
            .expect("query view existence");
        assert_eq!(view_exists, 0, "_spatia_step_1 should be dropped after execution");

        cleanup_temp_db(&db_path);
    }

    #[test]
    fn intermediate_views_cleaned_up_on_step_failure() {
        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute_batch(
            "CREATE TABLE data(val INTEGER); INSERT INTO data VALUES (1);",
        )
        .expect("setup table");
        drop(conn);

        // Step 1 creates a valid intermediate view; step 2 (final) references
        // a non-existent column to force a failure.
        let sql = "CREATE OR REPLACE VIEW _spatia_step_1 AS SELECT val FROM data; \
                   CREATE OR REPLACE VIEW analysis_result AS \
                       SELECT nonexistent_col FROM _spatia_step_1";

        let err = execute_analysis_sql_to_geojson(&db_path, sql)
            .expect_err("should fail on bad column reference");
        assert!(
            err.to_string().contains("Final step failed"),
            "error should identify which step failed: {err}"
        );

        // Verify intermediate views are cleaned up even though execution failed.
        let check_conn = Connection::open(&db_path).expect("open db for check");
        let view_exists: i32 = check_conn
            .query_row(
                "SELECT COUNT(*) FROM information_schema.tables \
                 WHERE table_schema = 'main' AND table_name = '_spatia_step_1'",
                [],
                |row| row.get(0),
            )
            .expect("query view existence");
        assert_eq!(view_exists, 0, "_spatia_step_1 should be dropped even on failure");

        cleanup_temp_db(&db_path);
    }

    /// Reproduce the UPDATE column-not-found issue with CSV-ingested tables.
    #[test]
    fn csv_ingested_table_update_finds_columns() {
        use std::io::Write;

        let db_path = temp_db_path();
        let csv_path = format!("{}.csv", &db_path[..db_path.len() - 7]);

        // Write a CSV similar to commercial_property_portfolio
        {
            let mut f = fs::File::create(&csv_path).expect("create csv");
            writeln!(f, "policy_number,insured_name,occupancy_code,stories,year_built,wildfire_score,distance_to_coast_mi,notes").unwrap();
            writeln!(f, "CPL-001,Acme Corp,851,3,1987,2,1.2,some note").unwrap();
            writeln!(f, "CPL-002,Beta Inc,623,2,2001,5,3.4,N/A").unwrap();
        }

        // Ingest via read_csv_auto (same as production ingest path)
        let conn = Connection::open(&db_path).expect("open db");
        conn.execute(
            &format!("CREATE TABLE test_portfolio AS SELECT * FROM read_csv_auto('{}')", csv_path),
            [],
        )
        .expect("create table from csv");

        // Verify schema has all columns
        let mut stmt = conn
            .prepare("SELECT column_name FROM information_schema.columns WHERE table_name = 'test_portfolio' ORDER BY ordinal_position")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        let mut col_names = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            let name: String = row.get(0).unwrap();
            col_names.push(name);
        }
        assert!(col_names.len() > 1, "Table should have multiple columns, got: {:?}", col_names);
        assert!(col_names.contains(&"occupancy_code".to_string()), "Should contain occupancy_code");

        // Try UPDATE with quoted column names (the pattern AI generates)
        let update_result = conn.execute_batch(
            r#"UPDATE test_portfolio SET "occupancy_code" = TRY_CAST("occupancy_code" AS INTEGER) WHERE "occupancy_code" IS NOT NULL;"#,
        );
        eprintln!("Quoted UPDATE result: {:?}", update_result);

        // Try UPDATE with unquoted column names
        let update_result2 = conn.execute_batch(
            "UPDATE test_portfolio SET occupancy_code = TRY_CAST(occupancy_code AS INTEGER) WHERE occupancy_code IS NOT NULL;",
        );
        eprintln!("Unquoted UPDATE result: {:?}", update_result2);

        // At least one should work
        assert!(
            update_result.is_ok() || update_result2.is_ok(),
            "UPDATE should work. Quoted: {:?}, Unquoted: {:?}",
            update_result, update_result2
        );

        let _ = fs::remove_file(&csv_path);
        cleanup_temp_db(&db_path);
    }

    /// Verify that UPDATE works on VARCHAR columns from a CSV-ingested table
    /// after the fallback ingestion path handles ragged CSVs.
    #[test]
    fn real_commercial_property_csv_update() {
        let csv_path = "/home/user/spatia/data/commercial_property_portfolio.csv";
        if !std::path::Path::new(csv_path).exists() {
            eprintln!("Skipping: CSV not found at {}", csv_path);
            return;
        }

        let db_path = temp_db_path();
        let conn = Connection::open(&db_path).expect("open db");

        // Use read_csv with explicit options (same as ingest fallback path)
        conn.execute(
            &format!(
                "CREATE OR REPLACE TABLE commercial_property_portfolio AS \
                 SELECT * FROM read_csv('{}', delim=',', header=true, auto_detect=true, null_padding=true)",
                csv_path
            ),
            [],
        ).expect("create table with explicit delim");

        // Verify we got the expected columns
        let col_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM information_schema.columns \
             WHERE table_name = 'commercial_property_portfolio'",
            [], |row| row.get(0),
        ).unwrap();
        assert!(col_count >= 30, "Expected 30+ columns, got {}", col_count);

        // Try UPDATEs on VARCHAR columns (the ones that failed in production)
        for col in &["occupancy_code", "notes", "flood_zone"] {
            let sql = format!(
                r#"UPDATE commercial_property_portfolio SET "{c}" = NULLIF(TRIM("{c}"), '') WHERE "{c}" IS NOT NULL;"#,
                c = col,
            );
            let result = conn.execute_batch(&sql);
            assert!(result.is_ok(), "UPDATE on {} failed: {:?}", col, result);
        }

        cleanup_temp_db(&db_path);
    }
}
