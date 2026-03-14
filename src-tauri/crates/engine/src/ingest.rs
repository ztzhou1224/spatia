use duckdb::Connection;

use crate::identifiers::validate_table_name;
use crate::EngineResult;

const RAW_STAGING_TABLE: &str = "raw_staging";

pub fn ingest_csv(db_path: &str, csv_path: &str) -> EngineResult<()> {
    let conn = Connection::open(db_path)?;
    ensure_spatial_extension(&conn)?;
    load_csv_to_table(&conn, csv_path, RAW_STAGING_TABLE, true)?;
    Ok(())
}

pub fn ingest_csv_to_table(db_path: &str, csv_path: &str, table_name: &str) -> EngineResult<()> {
    validate_table_name(table_name)?;
    let conn = Connection::open(db_path)?;
    ensure_spatial_extension(&conn)?;
    load_csv_to_table(&conn, csv_path, table_name, false)?;
    Ok(())
}

fn ensure_spatial_extension(conn: &Connection) -> EngineResult<()> {
    conn.execute("INSTALL spatial", [])?;
    conn.execute("LOAD spatial", [])?;
    Ok(())
}

fn load_csv_to_table(
    conn: &Connection,
    csv_path: &str,
    table_name: &str,
    replace: bool,
) -> EngineResult<()> {
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

#[cfg(test)]
mod tests {
    use super::{ingest_csv, ingest_csv_to_table};
    use crate::{raw_staging_schema, table_schema};
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn ingest_csv_loads_raw_staging_schema() {
        let (db_path, csv_path) = setup_files();
        ingest_csv(&db_path, &csv_path).expect("ingest_csv failed");
        let schema = raw_staging_schema(&db_path).expect("raw_staging_schema failed");
        let names: Vec<String> = schema.into_iter().map(|col| col.name).collect();
        assert_eq!(names, vec!["id", "name", "lat", "lon"]);
        cleanup_files(&db_path, &csv_path);
    }

    #[test]
    fn ingest_csv_to_table_loads_schema() {
        let (db_path, csv_path) = setup_files();
        ingest_csv_to_table(&db_path, &csv_path, "places").expect("ingest_csv_to_table failed");
        let schema = table_schema(&db_path, "places").expect("table_schema failed");
        let names: Vec<String> = schema.into_iter().map(|col| col.name).collect();
        assert_eq!(names, vec!["id", "name", "lat", "lon"]);
        cleanup_files(&db_path, &csv_path);
    }

    fn setup_files() -> (String, String) {
        let suffix = unique_suffix();
        let db_path = format!("/tmp/spatia_test_{suffix}.duckdb");
        let csv_path = format!("/tmp/spatia_test_{suffix}.csv");
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

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    }

    /// Verify that the fallback path handles CSVs that read_csv_auto
    /// collapses into a single column (e.g. the commercial property portfolio).
    #[test]
    fn ingest_fallback_for_real_csv() {
        let csv_path = "/home/user/spatia/data/commercial_property_portfolio.csv";
        if !std::path::Path::new(csv_path).exists() {
            eprintln!("Skipping: CSV not found");
            return;
        }

        let suffix = unique_suffix();
        let db_path = format!("/tmp/spatia_test_fallback_{suffix}.duckdb");

        // Use load_csv_to_table directly (skip spatial extension)
        let conn = duckdb::Connection::open(&db_path).expect("open db");
        super::load_csv_to_table(&conn, csv_path, "test_portfolio", false)
            .expect("load_csv_to_table should succeed with fallback");
        drop(conn);

        let schema = table_schema(&db_path, "test_portfolio")
            .expect("schema should be readable");
        let names: Vec<&str> = schema.iter().map(|c| c.name.as_str()).collect();
        eprintln!("Columns ({}): {:?}", names.len(), &names[..5.min(names.len())]);

        assert!(
            schema.len() >= 30,
            "Expected 30+ columns from commercial property CSV, got {}",
            schema.len()
        );
        assert!(
            names.contains(&"policy_number"),
            "Should contain policy_number column"
        );
        assert!(
            names.contains(&"occupancy_code"),
            "Should contain occupancy_code column"
        );

        // Only clean up the temp DB, NOT the original CSV
        let _ = fs::remove_file(&db_path);
        let _ = fs::remove_file(format!("{db_path}.wal"));
        let _ = fs::remove_file(format!("{db_path}.wal.lck"));
    }
}
