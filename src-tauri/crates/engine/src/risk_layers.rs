use duckdb::Connection;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLayerInfo {
    pub name: String,
    pub display_name: String,
    pub layer_type: String, // "flood", "wildfire", "wind", "custom"
    pub source: String,     // "FEMA NFHL", "USGS WHP", "user"
    pub row_count: usize,
    pub has_geometry: bool,
}

/// Ensure the risk_layer_registry metadata table exists.
pub fn ensure_risk_registry(conn: &Connection) -> Result<(), duckdb::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS risk_layer_registry (
            name VARCHAR PRIMARY KEY,
            display_name VARCHAR NOT NULL,
            layer_type VARCHAR NOT NULL,
            source VARCHAR NOT NULL,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );",
    )?;
    Ok(())
}

/// Ingest a spatial file (GeoJSON, Shapefile, etc.) as a risk layer.
pub fn ingest_risk_layer(
    conn: &Connection,
    file_path: &str,
    layer_name: &str,
    display_name: &str,
    layer_type: &str,
    source: &str,
) -> crate::EngineResult<RiskLayerInfo> {
    // Validate layer_name as safe SQL identifier.
    crate::validate_table_name(layer_name)?;

    // Load spatial extension.
    conn.execute_batch("INSTALL spatial; LOAD spatial;")?;

    // Create the risk layer table from the spatial file.
    let table_name = format!("risk_{}", layer_name);
    conn.execute_batch(&format!(
        "DROP TABLE IF EXISTS \"{table_name}\"; \
         CREATE TABLE \"{table_name}\" AS SELECT * FROM ST_Read('{file_path}');"
    ))?;

    // Get row count.
    let row_count: usize = conn.query_row(
        &format!("SELECT COUNT(*) FROM \"{table_name}\""),
        [],
        |row| row.get(0),
    )?;

    // Check if a geometry column exists by looking for GEOMETRY type or known geometry column names.
    let has_geometry = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM information_schema.columns \
                 WHERE table_name = '{table_name}' \
                 AND (data_type ILIKE '%GEOMETRY%' OR column_name = 'geom' OR column_name = 'geometry')"
            ),
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    // Register in the registry.
    ensure_risk_registry(conn)?;
    conn.execute(
        "INSERT OR REPLACE INTO risk_layer_registry (name, display_name, layer_type, source) \
         VALUES (?, ?, ?, ?)",
        duckdb::params![layer_name, display_name, layer_type, source],
    )?;

    info!(
        layer_name,
        display_name, row_count, "Risk layer ingested successfully"
    );

    Ok(RiskLayerInfo {
        name: layer_name.to_string(),
        display_name: display_name.to_string(),
        layer_type: layer_type.to_string(),
        source: source.to_string(),
        row_count,
        has_geometry,
    })
}

/// List all registered risk layers with their metadata.
pub fn list_risk_layers(conn: &Connection) -> crate::EngineResult<Vec<RiskLayerInfo>> {
    ensure_risk_registry(conn)?;

    // Collect registry entries first, then enrich with live row counts.
    // Doing it in two passes avoids a borrow conflict between `stmt` and inner
    // `conn.query_row` calls inside the same `query_map` closure.
    struct RegistryEntry {
        name: String,
        display_name: String,
        layer_type: String,
        source: String,
    }

    let mut stmt = conn.prepare(
        "SELECT name, display_name, layer_type, source \
         FROM risk_layer_registry \
         ORDER BY name",
    )?;

    let entries: Vec<RegistryEntry> = stmt
        .query_map([], |row| {
            Ok(RegistryEntry {
                name: row.get(0)?,
                display_name: row.get(1)?,
                layer_type: row.get(2)?,
                source: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // Drop stmt so `conn` is no longer borrowed.
    drop(stmt);

    let mut layers = Vec::with_capacity(entries.len());
    for entry in entries {
        let table_name = format!("risk_{}", entry.name);

        // Row count may be 0 if the table was dropped manually outside Spatia.
        let row_count = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM \"{table_name}\""),
                [],
                |r| r.get::<_, usize>(0),
            )
            .unwrap_or(0);

        layers.push(RiskLayerInfo {
            name: entry.name,
            display_name: entry.display_name,
            layer_type: entry.layer_type,
            source: entry.source,
            row_count,
            has_geometry: true,
        });
    }

    Ok(layers)
}

/// Remove a risk layer (drops the data table and its registry entry).
pub fn remove_risk_layer(conn: &Connection, layer_name: &str) -> crate::EngineResult<()> {
    crate::validate_table_name(layer_name)?;
    let table_name = format!("risk_{}", layer_name);
    conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{table_name}\";"  ))?;
    ensure_risk_registry(conn)?;
    conn.execute(
        "DELETE FROM risk_layer_registry WHERE name = ?",
        duckdb::params![layer_name],
    )?;
    info!(layer_name, "Risk layer removed");
    Ok(())
}

/// Export a risk layer as a GeoJSON FeatureCollection string for map rendering.
pub fn risk_layer_to_geojson(
    conn: &Connection,
    layer_name: &str,
    limit: Option<usize>,
) -> crate::EngineResult<String> {
    crate::validate_table_name(layer_name)?;
    let table_name = format!("risk_{}", layer_name);
    let lim = limit.unwrap_or(10_000);

    conn.execute_batch("LOAD spatial;")?;

    // Fetch column names and types.
    let mut col_stmt = conn.prepare(&format!(
        "SELECT column_name, data_type \
         FROM information_schema.columns \
         WHERE table_name = '{table_name}' \
         ORDER BY ordinal_position"
    ))?;

    let columns: Vec<(String, String)> = col_stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    drop(col_stmt);

    // Find the geometry column: prefer an explicit GEOMETRY type, fall back to
    // columns named "geom" or "geometry".
    let geom_col = columns
        .iter()
        .find(|(name, dtype)| {
            dtype.to_uppercase().contains("GEOMETRY")
                || name == "geom"
                || name == "geometry"
        })
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| "geom".to_string());

    // Property columns are everything except the geometry column.
    let prop_cols: Vec<&str> = columns
        .iter()
        .filter(|(name, dtype)| {
            !dtype.to_uppercase().contains("GEOMETRY") && name != "geom" && name != "geometry"
        })
        .map(|(name, _)| name.as_str())
        .collect();

    // Build the properties JSON expression using DuckDB's json_object().
    let prop_select = if prop_cols.is_empty() {
        "'{}'::JSON".to_string()
    } else {
        let fields: Vec<String> = prop_cols
            .iter()
            .map(|c| format!("'{c}', CAST(\"{c}\" AS VARCHAR)"))
            .collect();
        format!("json_object({})", fields.join(", "))
    };

    // ST_AsGeoJSON works directly on DuckDB GEOMETRY columns (no WKB conversion needed).
    let query = format!(
        "SELECT ST_AsGeoJSON(\"{geom_col}\") AS _geom, {prop_select} AS _props \
         FROM \"{table_name}\" \
         WHERE \"{geom_col}\" IS NOT NULL \
         LIMIT {lim}"
    );

    let mut stmt = conn.prepare(&query)?;
    let mut features: Vec<String> = Vec::new();

    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let geojson_str: String = row.get(0)?;
        let props_str: String = row.get(1)?;
        features.push(format!(
            r#"{{"type":"Feature","geometry":{geojson_str},"properties":{props_str}}}"#
        ));
    }

    let fc = format!(
        r#"{{"type":"FeatureCollection","features":[{}]}}"#,
        features.join(",")
    );

    Ok(fc)
}
