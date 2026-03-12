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
