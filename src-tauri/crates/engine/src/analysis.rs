use duckdb::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::EngineResult;

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisExecutionResult {
    pub status: &'static str,
    pub row_count: usize,
    pub geojson: Value,
}

pub fn execute_analysis_sql_to_geojson(
    db_path: &str,
    sql: &str,
) -> EngineResult<AnalysisExecutionResult> {
    validate_analysis_sql(sql)?;

    let conn = Connection::open(db_path)?;
    conn.execute_batch(sql)?;

    let mut schema_stmt = conn.prepare("PRAGMA table_info('analysis_result')")?;
    let schema_rows = schema_stmt.query_map([], |row| row.get::<_, String>(1))?;
    let col_names: Vec<String> = schema_rows.collect::<Result<Vec<_>, _>>()?;

    let mut stmt = conn.prepare("SELECT * FROM analysis_result LIMIT 1000")?;

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

        let lat = parse_number_property(&props, &["lat", "latitude"]);
        let lon = parse_number_property(&props, &["lon", "lng", "longitude"]);

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

    Ok(AnalysisExecutionResult {
        status: "ok",
        row_count: features.len(),
        geojson: json!({
            "type": "FeatureCollection",
            "features": features,
        }),
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

fn validate_analysis_sql(sql: &str) -> EngineResult<()> {
    let normalized = sql.trim().to_uppercase();
    if normalized.starts_with("CREATE OR REPLACE VIEW ANALYSIS_RESULT AS")
        || normalized.starts_with("CREATE VIEW ANALYSIS_RESULT AS")
    {
        return Ok(());
    }

    Err("analysis SQL must start with CREATE [OR REPLACE] VIEW analysis_result AS".into())
}

#[cfg(test)]
mod tests {
    use super::execute_analysis_sql_to_geojson;
    use duckdb::Connection;
    use serde_json::Value;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

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
}
