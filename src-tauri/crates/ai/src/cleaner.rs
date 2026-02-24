use duckdb::Connection;
use spatia_engine::{table_schema, TableColumn};

use crate::client::GeminiClient;
use crate::prompts::build_clean_prompt;
use crate::AiResult;

/// Number of sample rows fetched from the table when building the AI prompt.
const SAMPLE_ROW_COUNT: usize = 20;

/// The result of a cleaning run.
#[derive(Debug, Clone)]
pub struct CleanResult {
    /// The table that was cleaned.
    pub table: String,
    /// DuckDB `UPDATE` statements that were applied.
    pub statements_applied: Vec<String>,
    /// Column schema after the cleaning run (for callers to inspect type drift).
    pub schema_after: Vec<TableColumn>,
}

/// Fetch up to `SAMPLE_ROW_COUNT` rows from `table_name` and format them as
/// comma-separated lines (simple CSV-like text for the AI prompt).
fn fetch_sample_rows(conn: &Connection, table_name: &str) -> AiResult<String> {
    let sql = format!(
        "SELECT * FROM \"{table}\" USING SAMPLE {n} ROWS",
        table = table_name,
        n = SAMPLE_ROW_COUNT,
    );
    let mut stmt = conn.prepare(&sql)?;
    let col_count = stmt.column_count();

    let mut lines: Vec<String> = Vec::new();
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let mut cells: Vec<String> = Vec::with_capacity(col_count);
        for i in 0..col_count {
            let val: Option<String> = row.get(i).ok();
            cells.push(val.unwrap_or_default());
        }
        lines.push(cells.join(","));
    }
    Ok(lines.join("\n"))
}

/// Extract bare SQL statements from the AI response.
///
/// The model is instructed to return one statement per line with no markdown,
/// but we strip any accidental fences or blank lines defensively.
fn extract_sql_statements(ai_text: &str) -> Vec<String> {
    ai_text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("```") && !l.starts_with("--"))
        .map(str::to_owned)
        .collect()
}

/// Validate that a SQL statement is a safe `UPDATE` statement before execution.
///
/// Only `UPDATE` statements are permitted; DDL and other DML commands are
/// rejected to prevent the AI from accidentally (or maliciously) deleting data
/// or altering schema.
fn validate_statement(stmt: &str) -> AiResult<()> {
    let upper = stmt.trim_start().to_uppercase();
    if !upper.starts_with("UPDATE ") {
        return Err(format!(
            "unsafe statement rejected (only UPDATE is allowed): {}",
            stmt
        )
        .into());
    }
    Ok(())
}

/// Clean the data in `table_name` using the Gemini AI.
///
/// Steps:
/// 1. Open `db_path` and fetch the table schema + up to 20 sample rows.
/// 2. Build a cleaning prompt and call the Gemini model.
/// 3. Parse the returned SQL statements and execute each one.
/// 4. Re-fetch the schema and return a [`CleanResult`].
pub async fn clean_table(
    db_path: &str,
    table_name: &str,
    client: &GeminiClient,
) -> AiResult<CleanResult> {
    // 1. Fetch schema and sample rows via a single connection.
    let schema = table_schema(db_path, table_name)?;
    let conn = Connection::open(db_path)?;
    let sample_rows = fetch_sample_rows(&conn, table_name)?;

    // 2. Build prompt and call the AI.
    let prompt = build_clean_prompt(table_name, &schema, &sample_rows);
    let ai_response = client.generate(&prompt).await?;

    // 3. Execute each returned UPDATE statement (allowlisted to UPDATE only).
    let statements = extract_sql_statements(&ai_response);
    let mut applied: Vec<String> = Vec::new();
    for stmt in &statements {
        validate_statement(stmt)?;
        conn.execute_batch(stmt)?;
        applied.push(stmt.clone());
    }

    // 4. Re-fetch schema for caller inspection.
    let schema_after = table_schema(db_path, table_name)?;

    Ok(CleanResult {
        table: table_name.to_string(),
        statements_applied: applied,
        schema_after,
    })
}

#[cfg(test)]
mod tests {
    use super::{extract_sql_statements, validate_statement};

    #[test]
    fn strips_markdown_fences_and_blanks() {
        let ai_text = r#"
```sql
UPDATE foo SET bar = TRIM(bar);
```

UPDATE foo SET baz = LOWER(baz);
"#;
        let stmts = extract_sql_statements(ai_text);
        // Both UPDATE lines are kept; only fence markers and blank lines are removed.
        assert_eq!(stmts.len(), 2);
        assert!(stmts.iter().any(|s| s.contains("TRIM(bar)")));
        assert!(stmts.iter().any(|s| s.contains("LOWER(baz)")));
    }

    #[test]
    fn strips_comment_only_response() {
        let ai_text = "-- no changes needed";
        let stmts = extract_sql_statements(ai_text);
        assert!(stmts.is_empty());
    }

    #[test]
    fn returns_multiple_statements() {
        let ai_text = "UPDATE t SET a = TRIM(a);\nUPDATE t SET b = UPPER(b);";
        let stmts = extract_sql_statements(ai_text);
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn validate_rejects_non_update_statements() {
        assert!(validate_statement("DROP TABLE foo").is_err());
        assert!(validate_statement("DELETE FROM foo").is_err());
        assert!(validate_statement("SELECT * FROM foo").is_err());
        assert!(validate_statement("UPDATE foo SET a = 1").is_ok());
        // case-insensitive prefix check
        assert!(validate_statement("update foo SET a = 1").is_ok());
    }
}
