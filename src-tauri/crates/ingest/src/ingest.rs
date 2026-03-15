use std::path::Path;

use duckdb::Connection;

use crate::identifiers::validate_table_name;
use crate::IngestResult;

const RAW_STAGING_TABLE: &str = "raw_staging";

/// Supported spatial file extensions (case-insensitive check).
const SPATIAL_EXTENSIONS: &[&str] = &["geojson", "json", "shp", "gpkg", "fgb"];

/// Returns `true` when the file path has a spatial-file extension.
pub fn is_spatial_file(file_path: &str) -> bool {
    Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SPATIAL_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn ingest_csv(db_path: &str, csv_path: &str) -> IngestResult<()> {
    let conn = Connection::open(db_path)?;
    ensure_spatial_extension(&conn)?;
    load_csv_to_table(&conn, csv_path, RAW_STAGING_TABLE, true)?;
    Ok(())
}

pub fn ingest_csv_to_table(db_path: &str, csv_path: &str, table_name: &str) -> IngestResult<()> {
    validate_table_name(table_name)?;
    let conn = Connection::open(db_path)?;
    ensure_spatial_extension(&conn)?;
    load_csv_to_table(&conn, csv_path, table_name, false)?;
    Ok(())
}

/// Ingest a spatial file (GeoJSON, Shapefile, GeoPackage, FlatGeobuf) into DuckDB
/// using `ST_Read()` from the spatial extension. The geometry column is stored as
/// DuckDB's native GEOMETRY type.
pub fn ingest_spatial_file(db_path: &str, file_path: &str, table_name: &str) -> IngestResult<()> {
    validate_table_name(table_name)?;
    let conn = Connection::open(db_path)?;
    ensure_spatial_extension(&conn)?;
    load_spatial_to_table(&conn, file_path, table_name)?;
    Ok(())
}

fn ensure_spatial_extension(conn: &Connection) -> IngestResult<()> {
    conn.execute("INSTALL spatial", [])?;
    conn.execute("LOAD spatial", [])?;
    Ok(())
}

fn load_csv_to_table(
    conn: &Connection,
    csv_path: &str,
    table_name: &str,
    replace: bool,
) -> IngestResult<()> {
    let escaped_csv_path = csv_path.replace('\'', "''");
    let create = if replace { "CREATE OR REPLACE TABLE" } else { "CREATE TABLE" };

    // Try read_csv_auto first; if it produces only 1 column (delimiter
    // mis-detection), fall back to read_csv with explicit comma delimiter
    // and null_padding for ragged rows.
    let auto_sql = format!(
        "{create} {table} AS SELECT * FROM read_csv_auto('{csv}')",
        create = create, table = table_name, csv = escaped_csv_path,
    );
    conn.execute(&auto_sql, [])?;

    let col_count: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM information_schema.columns \
             WHERE table_schema = 'main' AND table_name = '{}'",
            table_name.replace('\'', "''")
        ),
        [],
        |row| row.get(0),
    )?;

    if col_count <= 1 {
        // read_csv_auto failed to detect delimiter; retry with explicit options
        tracing::warn!(
            table = %table_name,
            auto_col_count = col_count,
            "load_csv_to_table: read_csv_auto produced single column, retrying with explicit delimiter"
        );
        let fallback_sql = format!(
            "CREATE OR REPLACE TABLE {table} AS SELECT * FROM read_csv('{csv}', \
             delim=',', header=true, auto_detect=true, null_padding=true)",
            table = table_name, csv = escaped_csv_path,
        );
        conn.execute(&fallback_sql, [])?;
    }

    Ok(())
}

fn load_spatial_to_table(
    conn: &Connection,
    file_path: &str,
    table_name: &str,
) -> IngestResult<()> {
    let escaped_path = file_path.replace('\'', "''");

    // ST_Read reads GeoJSON, Shapefile, GPKG, FGB, and other GDAL-supported formats.
    // The resulting table includes a `geom` (or `geometry`) column of DuckDB GEOMETRY type.
    let sql = format!(
        "CREATE OR REPLACE TABLE {table} AS SELECT * FROM ST_Read('{path}')",
        table = table_name,
        path = escaped_path,
    );
    conn.execute(&sql, [])?;

    tracing::info!(
        table = %table_name,
        file = %file_path,
        "ingest_spatial_file: loaded spatial file via ST_Read"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ingest_csv, ingest_csv_to_table, ingest_spatial_file, is_spatial_file};
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn ingest_csv_loads_raw_staging_schema() {
        let (db_path, csv_path) = setup_files();
        ingest_csv(&db_path, &csv_path).expect("ingest_csv failed");
        // Verify table was created by querying column count
        let conn = duckdb::Connection::open(&db_path).expect("open db");
        let col_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM information_schema.columns \
                 WHERE table_schema = 'main' AND table_name = 'raw_staging'",
                [],
                |row| row.get(0),
            )
            .expect("count columns");
        assert_eq!(col_count, 4);
        cleanup_files(&db_path, &csv_path);
    }

    #[test]
    fn ingest_csv_to_table_loads_schema() {
        let (db_path, csv_path) = setup_files();
        ingest_csv_to_table(&db_path, &csv_path, "places").expect("ingest_csv_to_table failed");
        let conn = duckdb::Connection::open(&db_path).expect("open db");
        let col_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM information_schema.columns \
                 WHERE table_schema = 'main' AND table_name = 'places'",
                [],
                |row| row.get(0),
            )
            .expect("count columns");
        assert_eq!(col_count, 4);
        cleanup_files(&db_path, &csv_path);
    }

    fn setup_files() -> (String, String) {
        let suffix = unique_suffix();
        let db_path = format!("/tmp/spatia_ingest_test_{suffix}.duckdb");
        let csv_path = format!("/tmp/spatia_ingest_test_{suffix}.csv");
        let mut file = fs::File::create(&csv_path).expect("create csv");
        writeln!(file, "id,name,lat,lon").expect("write header");
        writeln!(file, "1,City Hall,37.7793,-122.4192").expect("write row");
        (db_path, csv_path)
    }

    fn cleanup_files(db_path: &str, csv_path: &str) {
        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(format!("{db_path}.wal"));
        let _ = fs::remove_file(format!("{db_path}.wal.lck"));
        let _ = fs::remove_file(csv_path);
    }

    #[test]
    fn is_spatial_file_detects_extensions() {
        assert!(is_spatial_file("data/zones.geojson"));
        assert!(is_spatial_file("data/zones.GeoJSON"));
        assert!(is_spatial_file("data/zones.json"));
        assert!(is_spatial_file("data/zones.shp"));
        assert!(is_spatial_file("data/zones.gpkg"));
        assert!(is_spatial_file("data/zones.fgb"));
        assert!(!is_spatial_file("data/zones.csv"));
        assert!(!is_spatial_file("data/zones.txt"));
        assert!(!is_spatial_file("data/zones"));
    }

    #[test]
    fn ingest_spatial_file_loads_geojson() {
        let suffix = unique_suffix();
        let db_path = format!("/tmp/spatia_spatial_test_{suffix}.duckdb");
        let geojson_path = format!("/tmp/spatia_spatial_test_{suffix}.geojson");

        // Write a minimal GeoJSON FeatureCollection
        let geojson = r#"{
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "geometry": { "type": "Point", "coordinates": [-122.4, 37.8] },
                    "properties": { "name": "Point A", "value": 42 }
                },
                {
                    "type": "Feature",
                    "geometry": { "type": "Polygon", "coordinates": [[[-122.5, 37.7], [-122.3, 37.7], [-122.3, 37.9], [-122.5, 37.9], [-122.5, 37.7]]] },
                    "properties": { "name": "Zone B", "value": 99 }
                }
            ]
        }"#;
        fs::write(&geojson_path, geojson).expect("write geojson");

        ingest_spatial_file(&db_path, &geojson_path, "test_spatial")
            .expect("ingest_spatial_file failed");

        let conn = duckdb::Connection::open(&db_path).expect("open db");
        conn.execute("LOAD spatial", []).expect("load spatial");

        // Verify row count
        let row_count: i64 = conn
            .query_row(
                r#"SELECT COUNT(*) FROM "test_spatial""#,
                [],
                |row| row.get(0),
            )
            .expect("count rows");
        assert_eq!(row_count, 2);

        // Verify geometry column exists (ST_Read names it "geom" by default)
        let has_geom: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM information_schema.columns \
                 WHERE table_name = 'test_spatial' AND column_name = 'geom'",
                [],
                |row| row.get(0),
            )
            .expect("check geom column");
        assert!(has_geom, "Expected 'geom' column from ST_Read");

        // Cleanup
        let _ = fs::remove_file(&db_path);
        let _ = fs::remove_file(format!("{db_path}.wal"));
        let _ = fs::remove_file(format!("{db_path}.wal.lck"));
        let _ = fs::remove_file(&geojson_path);
    }

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    }
}
