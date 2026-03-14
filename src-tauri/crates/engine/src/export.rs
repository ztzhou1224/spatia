use duckdb::Connection;
use serde_json::{json, Map, Value};
use tracing::info;

use crate::identifiers::validate_table_name;
use crate::EngineResult;

/// Export a DuckDB table as CSV to the given file path.
pub fn export_table_csv(conn: &Connection, table_name: &str, file_path: &str) -> EngineResult<()> {
    validate_table_name(table_name)?;
    let escaped_path = file_path.replace('\'', "''");
    let sql = format!(r#"COPY "{table_name}" TO '{escaped_path}' (FORMAT CSV, HEADER)"#);
    conn.execute_batch(&sql)?;
    info!(table = %table_name, path = %file_path, "export_table_csv: exported successfully");
    Ok(())
}

/// Export the `analysis_result` view as a GeoJSON FeatureCollection to the given file path.
pub fn export_analysis_geojson(conn: &Connection, file_path: &str) -> EngineResult<()> {
    // Get column names
    let mut schema_stmt = conn.prepare(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema = 'main' AND table_name = 'analysis_result' \
         ORDER BY ordinal_position",
    )?;
    let mut schema_rows = schema_stmt.query([])?;
    let mut col_names: Vec<String> = Vec::new();
    while let Some(row) = schema_rows.next()? {
        col_names.push(row.get::<_, String>(0)?);
    }

    if col_names.is_empty() {
        return Err("analysis_result view does not exist or has no columns".into());
    }

    let cast_select = col_names
        .iter()
        .map(|c| format!(r#"CAST("{c}" AS VARCHAR) AS "{c}""#))
        .collect::<Vec<_>>()
        .join(", ");

    // Query all rows (no LIMIT for export)
    let mut stmt = conn.prepare(&format!("SELECT {cast_select} FROM analysis_result"))?;
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

        let lat = parse_coord(&props, &["lat", "latitude", "_lat"]);
        let lon = parse_coord(&props, &["lon", "lng", "longitude", "_lon"]);

        let geometry = match (lat, lon) {
            (Some(lat), Some(lon)) => json!({ "type": "Point", "coordinates": [lon, lat] }),
            _ => Value::Null,
        };

        features.push(json!({
            "type": "Feature",
            "geometry": geometry,
            "properties": Value::Object(props),
        }));
    }

    let fc = json!({
        "type": "FeatureCollection",
        "features": features,
    });

    std::fs::write(file_path, serde_json::to_string_pretty(&fc)?)?;
    info!(features = features.len(), path = %file_path, "export_analysis_geojson: exported successfully");
    Ok(())
}

fn parse_coord(props: &Map<String, Value>, names: &[&str]) -> Option<f64> {
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
