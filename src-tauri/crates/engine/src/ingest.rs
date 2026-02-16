use duckdb::Connection;

use crate::identifiers::validate_table_name;
use crate::EngineResult;

pub fn ingest_csv_to_table(db_path: &str, csv_path: &str, table_name: &str) -> EngineResult<()> {
    validate_table_name(table_name)?;
    let conn = Connection::open(db_path)?;
    let escaped_csv_path = csv_path.replace('\'', "''");
    let sql = format!(
        "CREATE TABLE {table} AS SELECT * FROM read_csv_auto('{csv}')",
        table = table_name,
        csv = escaped_csv_path
    );
    conn.execute(&sql, [])?;
    Ok(())
}
