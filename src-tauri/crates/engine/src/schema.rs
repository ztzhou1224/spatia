use serde::{Deserialize, Serialize};

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
    validate_table_name(table_name)?;
    let conn = Connection::open(db_path)?;
    let sql = format!("PRAGMA table_info('{table}')", table = table_name);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let notnull: bool = row.get(3)?;
        let pk: bool = row.get(5)?;
        Ok(TableColumn {
            cid: row.get(0)?,
            name: row.get(1)?,
            data_type: row.get(2)?,
            notnull,
            default_value: row.get(4)?,
            primary_key: pk,
        })
    })?;

    let mut columns = Vec::new();
    for row in rows {
        columns.push(row?);
    }
    Ok(columns)
}

pub fn raw_staging_schema(db_path: &str) -> EngineResult<Vec<TableColumn>> {
    table_schema(db_path, "raw_staging")
}
