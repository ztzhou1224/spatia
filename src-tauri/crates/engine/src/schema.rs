use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::identifiers::validate_table_name;
use crate::EngineResult;
use duckdb::Connection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumn {
    pub cid: i64,
    pub name: String,
    pub data_type: String,
    pub notnull: bool,
    pub default_value: Option<String>,
    pub primary_key: bool,
}

pub fn table_schema(db_path: &str, table_name: &str) -> EngineResult<Vec<TableColumn>> {
    debug!(table = %table_name, "table_schema: fetching schema");
    validate_table_name(table_name)?;
    let conn = Connection::open(db_path)?;

    // Use information_schema with query() (not query_map) to avoid DuckDB
    // 1.4.4 Rust driver panic on column_count() before statement execution.
    let sql = format!(
        "SELECT ordinal_position - 1, column_name, data_type, \
               CASE WHEN is_nullable = 'NO' THEN true ELSE false END, \
               column_default \
         FROM information_schema.columns \
         WHERE table_schema = 'main' AND table_name = '{}' \
         ORDER BY ordinal_position",
        table_name.replace('\'', "''")
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut columns = Vec::new();
    while let Some(row) = rows.next()? {
        columns.push(TableColumn {
            cid: row.get(0)?,
            name: row.get(1)?,
            data_type: row.get(2)?,
            notnull: row.get(3)?,
            default_value: row.get(4)?,
            primary_key: false,
        });
    }
    info!(table = %table_name, column_count = columns.len(), "table_schema: fetched successfully");
    Ok(columns)
}

pub fn raw_staging_schema(db_path: &str) -> EngineResult<Vec<TableColumn>> {
    table_schema(db_path, "raw_staging")
}

/// Maximum number of distinct values to consider a column "low-cardinality".
const MAX_DISTINCT_FOR_SAMPLES: usize = 20;
/// Maximum number of sample values to return per column.
const SAMPLE_VALUES_LIMIT: usize = 10;

/// Fetch sample distinct values for low-cardinality VARCHAR/TEXT columns.
///
/// Returns a map of column_name → vec of distinct non-NULL values (up to
/// `SAMPLE_VALUES_LIMIT`). Only columns with ≤ `MAX_DISTINCT_FOR_SAMPLES`
/// distinct values are included — this avoids injecting high-cardinality
/// columns (like names or addresses) into the prompt.
pub fn fetch_column_samples(
    db_path: &str,
    table_name: &str,
) -> EngineResult<HashMap<String, Vec<String>>> {
    validate_table_name(table_name)?;
    let conn = Connection::open(db_path)?;
    let schema = table_schema(db_path, table_name)?;

    let mut samples: HashMap<String, Vec<String>> = HashMap::new();

    for col in &schema {
        let dtype = col.data_type.to_uppercase();
        if !dtype.contains("VARCHAR") && !dtype.contains("TEXT") && !dtype.contains("ENUM") {
            continue;
        }

        // Count distinct non-NULL values; skip if too many.
        let count_sql = format!(
            "SELECT COUNT(DISTINCT \"{col}\") FROM \"{table}\" WHERE \"{col}\" IS NOT NULL",
            col = col.name,
            table = table_name,
        );
        let distinct_count: u64 = match conn.query_row(&count_sql, [], |row| row.get(0)) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if distinct_count == 0 || distinct_count as usize > MAX_DISTINCT_FOR_SAMPLES {
            continue;
        }

        // Fetch the actual values
        let fetch_sql = format!(
            "SELECT DISTINCT \"{col}\" FROM \"{table}\" WHERE \"{col}\" IS NOT NULL ORDER BY \"{col}\" LIMIT {limit}",
            col = col.name,
            table = table_name,
            limit = SAMPLE_VALUES_LIMIT,
        );
        let mut stmt = conn.prepare(&fetch_sql)?;
        let mut rows = stmt.query([])?;
        let mut values = Vec::new();
        while let Some(row) = rows.next()? {
            let val: String = row.get(0)?;
            values.push(val);
        }
        if !values.is_empty() {
            debug!(table = %table_name, column = %col.name, count = values.len(), "fetch_column_samples: found sample values");
            samples.insert(col.name.clone(), values);
        }
    }

    info!(table = %table_name, columns_with_samples = samples.len(), "fetch_column_samples: complete");
    Ok(samples)
}
