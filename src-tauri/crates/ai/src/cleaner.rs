use duckdb::Connection;
use spatia_engine::{table_schema, TableColumn};
use tracing::{debug, error, info, warn};

use crate::client::GeminiClient;
use crate::prompts::{build_clean_prompt, build_clean_retry_prompt};
use crate::AiResult;

/// Number of sample rows fetched from the table when building the AI prompt.
const SAMPLE_ROW_COUNT: usize = 20;
const RAW_STAGING_TABLE: &str = "raw_staging";
/// Maximum number of cleaning rounds before stopping regardless of AI suggestions.
const MAX_CLEAN_ROUNDS: usize = 3;

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

/// Common null-sentinel strings that should pass through masking unmodified,
/// since they represent data-quality patterns the AI needs to detect.
const NULL_SENTINELS: &[&str] = &[
    "N/A", "n/a", "NA", "na", "null", "NULL", "none", "None", "NONE", "–", "-", "..",
];

/// Mask a single cell value using character-class replacement.
///
/// - Uppercase letters → `X`
/// - Lowercase letters → `x`
/// - Digits → `9`
/// - Everything else (spaces, punctuation, `$`, `,`, `.`) preserved as-is
///
/// Null sentinel values (e.g. `"N/A"`, `"null"`) pass through unmasked so the
/// AI can still detect them.
fn mask_cell(value: &str) -> String {
    if NULL_SENTINELS.contains(&value.trim()) {
        return value.to_string();
    }
    value
        .chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                'X'
            } else if c.is_ascii_lowercase() {
                'x'
            } else if c.is_ascii_digit() {
                '9'
            } else {
                c
            }
        })
        .collect()
}

/// Mask all sample rows so real data values are not sent to the AI.
///
/// Preserves the structure (whitespace, casing pattern, punctuation, format)
/// so the AI can still identify data-quality issues.
fn mask_sample_rows(raw: &str) -> String {
    raw.lines()
        .map(|line| {
            line.split(',')
                .map(mask_cell)
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Fetch up to `SAMPLE_ROW_COUNT` rows from `table_name` and format them as
/// comma-separated lines (simple CSV-like text for the AI prompt).
fn fetch_sample_rows(conn: &Connection, table_name: &str) -> AiResult<String> {
    let sql = format!(
        "SELECT * FROM \"{table}\" USING SAMPLE {n} ROWS",
        table = table_name,
        n = SAMPLE_ROW_COUNT,
    );
    debug!(table = %table_name, sql = %sql, "fetch_sample_rows: preparing sample query");
    let mut stmt = conn.prepare(&sql).map_err(|e| {
        error!(table = %table_name, sql = %sql, error = %e, "fetch_sample_rows: failed to prepare statement");
        e
    })?;
    // NOTE: column_count() panics if called before query() in duckdb-rs 1.4.4
    // (it requires the statement to have been executed first). Execute first,
    // then obtain the column count from the Rows handle which wraps the
    // already-executed statement.
    let mut rows = stmt.query([])?;
    // Obtain the column count from the now-executed statement via the Rows ref.
    let col_count = rows.as_ref().map(|s| s.column_count()).unwrap_or(0);

    let mut lines: Vec<String> = Vec::new();
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
/// but in practice it often returns multi-line SQL. We therefore join all
/// non-fence, non-comment lines into a single string and split on semicolons
/// to recover individual statements. This correctly handles both the single-
/// line-per-statement format and multi-line SQL blocks with lambda expressions.
fn extract_sql_statements(ai_text: &str) -> Vec<String> {
    // Strip markdown fences and pure comment lines, then join everything.
    let joined: String = ai_text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("```") && !l.starts_with("--"))
        .collect::<Vec<_>>()
        .join(" ");

    // Split on semicolons; keep only non-empty chunks.
    joined
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("{s};"))
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

/// Validate that column type declarations are unchanged after cleanup updates.
fn validate_schema_types(before: &[TableColumn], after: &[TableColumn]) -> AiResult<()> {
    if before.len() != after.len() {
        return Err("schema changed during cleanup (column count mismatch)".into());
    }

    for (before_col, after_col) in before.iter().zip(after.iter()) {
        if before_col.name != after_col.name {
            return Err(format!(
                "schema changed during cleanup (column order/name mismatch: expected '{}' found '{}')",
                before_col.name, after_col.name
            )
            .into());
        }
        if before_col.data_type != after_col.data_type {
            return Err(format!(
                "column type changed during cleanup for '{}': '{}' -> '{}'",
                before_col.name, before_col.data_type, after_col.data_type
            )
            .into());
        }
    }

    Ok(())
}

/// Clean the default ingestion table (`raw_staging`) using the Gemini AI.
pub async fn clean_raw_staging(db_path: &str, client: &GeminiClient) -> AiResult<CleanResult> {
    clean_table(db_path, RAW_STAGING_TABLE, client).await
}

/// Execute a single batch of AI-generated UPDATE statements.
///
/// Failed statements get one AI-assisted retry. Returns (applied, failed_prompts)
/// where `failed_prompts` carries (original_stmt, error) pairs that need AI retry
/// — but retry is handled by the caller to keep this fn synchronous and thus
/// avoid holding a `Connection` across `.await` points.
///
/// Returns the list of successfully applied statements.
fn try_execute_statements(
    conn: &Connection,
    table_name: &str,
    statements: &[String],
) -> (Vec<String>, Vec<(String, String)>) {
    let mut applied: Vec<String> = Vec::new();
    let mut needs_retry: Vec<(String, String)> = Vec::new();

    for stmt in statements {
        // Reject non-UPDATE statements immediately — no retry for safety violations.
        if let Err(e) = validate_statement(stmt) {
            warn!(table = %table_name, sql = %stmt, error = %e, "clean_table: skipping unsafe statement");
            continue;
        }

        debug!(table = %table_name, sql = %stmt, "clean_table: executing UPDATE statement");
        match conn.execute_batch(stmt) {
            Ok(()) => {
                applied.push(stmt.clone());
            }
            Err(err) => {
                let err_msg = err.to_string();
                warn!(
                    table = %table_name,
                    sql = %stmt,
                    error = %err_msg,
                    "clean_table: UPDATE failed, will request AI retry"
                );
                needs_retry.push((stmt.clone(), err_msg));
            }
        }
    }

    (applied, needs_retry)
}

/// Clean the data in `table_name` using the Gemini AI.
///
/// Runs up to `MAX_CLEAN_ROUNDS` rounds of AI-driven cleaning. Each round:
/// 1. Fetches fresh sample rows.
/// 2. Asks the AI for UPDATE statements.
/// 3. Executes them; for any that fail, asks AI to fix and retries once.
///
/// Stops early if a round applies 0 statements (AI found nothing left to fix).
///
/// Note: `Connection` is not `Send`, so it is opened, used synchronously, and
/// dropped before every `.await` point to keep the future `Send`.
pub async fn clean_table(
    db_path: &str,
    table_name: &str,
    client: &GeminiClient,
) -> AiResult<CleanResult> {
    info!(table = %table_name, max_rounds = MAX_CLEAN_ROUNDS, "clean_table: starting AI clean");

    let schema = table_schema(db_path, table_name)?;

    let mut all_applied: Vec<String> = Vec::new();

    for round in 1..=MAX_CLEAN_ROUNDS {
        // Open a connection, do all synchronous work, then drop it before awaiting.
        let prompt = {
            let conn = Connection::open(db_path)?;
            let rows = fetch_sample_rows(&conn, table_name)?;
            let masked = mask_sample_rows(&rows);
            build_clean_prompt(table_name, &schema, &masked)
        };

        debug!(
            table = %table_name,
            round = round,
            prompt_len = prompt.len(),
            "clean_table: sending prompt to Gemini"
        );

        // Await AI — connection is NOT held here.
        let ai_response = client.generate(&prompt).await.map_err(|e| {
            error!(table = %table_name, round = round, error = %e, "clean_table: Gemini API call failed");
            e
        })?;
        debug!(
            table = %table_name,
            round = round,
            response_len = ai_response.len(),
            "clean_table: received Gemini response"
        );

        let statements = extract_sql_statements(&ai_response);
        debug!(
            table = %table_name,
            round = round,
            statement_count = statements.len(),
            "clean_table: extracted SQL statements"
        );

        if statements.is_empty() {
            info!(table = %table_name, round = round, "clean_table: no statements from AI, stopping early");
            break;
        }

        // Execute synchronously; collect any that need AI-assisted retry.
        let (round_applied, needs_retry) = {
            let conn = Connection::open(db_path)?;
            try_execute_statements(&conn, table_name, &statements)
        };

        // For each failed statement, ask AI to fix it, then try once more.
        // We build all retry prompts first, then await them, then execute.
        let mut retry_corrected: Vec<String> = Vec::new();
        for (failed_stmt, err_msg) in &needs_retry {
            let retry_prompt = build_clean_retry_prompt(failed_stmt, err_msg);
            match client.generate(&retry_prompt).await {
                Err(api_err) => {
                    warn!(
                        table = %table_name,
                        sql = %failed_stmt,
                        error = %api_err,
                        "clean_table: AI retry request failed, skipping statement"
                    );
                }
                Ok(retry_response) => {
                    let retry_stmts = extract_sql_statements(&retry_response);
                    let corrected = retry_stmts.into_iter().next().unwrap_or_default();
                    if corrected.is_empty() {
                        warn!(
                            table = %table_name,
                            sql = %failed_stmt,
                            "clean_table: AI returned no corrected statement, skipping"
                        );
                        continue;
                    }
                    if let Err(e) = validate_statement(&corrected) {
                        warn!(
                            table = %table_name,
                            sql = %corrected,
                            error = %e,
                            "clean_table: AI retry returned unsafe statement, skipping"
                        );
                        continue;
                    }
                    retry_corrected.push(corrected);
                }
            }
        }

        // Execute corrected retry statements synchronously (no await held).
        let mut retry_applied: Vec<String> = Vec::new();
        if !retry_corrected.is_empty() {
            let conn = Connection::open(db_path)?;
            for corrected in &retry_corrected {
                debug!(
                    table = %table_name,
                    corrected_sql = %corrected,
                    "clean_table: executing AI-corrected UPDATE statement"
                );
                match conn.execute_batch(corrected) {
                    Ok(()) => {
                        info!(
                            table = %table_name,
                            corrected_sql = %corrected,
                            "clean_table: AI retry succeeded"
                        );
                        retry_applied.push(corrected.clone());
                    }
                    Err(retry_err) => {
                        warn!(
                            table = %table_name,
                            corrected_sql = %corrected,
                            error = %retry_err,
                            "clean_table: AI retry also failed, skipping statement"
                        );
                    }
                }
            }
        }

        let total_round_applied = round_applied.len() + retry_applied.len();
        info!(
            table = %table_name,
            round = round,
            statements_applied = total_round_applied,
            "clean_table: round complete"
        );

        // If the AI produced statements but none succeeded, further rounds won't help.
        if total_round_applied == 0 {
            info!(
                table = %table_name,
                round = round,
                "clean_table: 0 statements applied this round, stopping early"
            );
            break;
        }

        all_applied.extend(round_applied);
        all_applied.extend(retry_applied);
    }

    // Re-fetch schema for caller inspection and type-drift validation.
    let schema_after = table_schema(db_path, table_name)?;
    if let Err(e) = validate_schema_types(&schema, &schema_after) {
        warn!(table = %table_name, error = %e, "clean_table: schema type validation failed after cleaning");
        return Err(e);
    }

    info!(
        table = %table_name,
        total_statements_applied = all_applied.len(),
        "clean_table: completed successfully"
    );
    Ok(CleanResult {
        table: table_name.to_string(),
        statements_applied: all_applied,
        schema_after,
    })
}

#[cfg(test)]
mod tests {
    use super::{extract_sql_statements, mask_cell, mask_sample_rows, validate_schema_types, validate_statement};
    use spatia_engine::TableColumn;

    fn col(name: &str, data_type: &str, cid: i64) -> TableColumn {
        TableColumn {
            cid,
            name: name.to_string(),
            data_type: data_type.to_string(),
            notnull: false,
            default_value: None,
            primary_key: false,
        }
    }

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
    fn joins_multiline_sql_into_single_statement() {
        // The AI often returns multi-line SQL like the title-case recipe.
        // The old line-split approach would shred this into fragments.
        let ai_text = r#"UPDATE t SET city = ARRAY_TO_STRING(
    LIST_TRANSFORM(
        STRING_SPLIT(LOWER(TRIM(city)), ' '),
        x -> CONCAT(UPPER(x[1]), x[2:])
    ), ' ')
WHERE city IS NOT NULL;"#;
        let stmts = extract_sql_statements(ai_text);
        assert_eq!(stmts.len(), 1, "multi-line SQL should parse as one statement");
        assert!(stmts[0].starts_with("UPDATE t SET city"));
        assert!(stmts[0].contains("ARRAY_TO_STRING"));
        assert!(stmts[0].contains("LIST_TRANSFORM"));
        assert!(stmts[0].contains("WHERE city IS NOT NULL"));
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

    #[test]
    fn schema_type_validation_accepts_unchanged_schema() {
        let before = vec![col("city", "VARCHAR", 0), col("count", "INTEGER", 1)];
        let after = vec![col("city", "VARCHAR", 0), col("count", "INTEGER", 1)];
        assert!(validate_schema_types(&before, &after).is_ok());
    }

    #[test]
    fn schema_type_validation_rejects_type_change() {
        let before = vec![col("count", "INTEGER", 0)];
        let after = vec![col("count", "VARCHAR", 0)];
        assert!(validate_schema_types(&before, &after).is_err());
    }

    #[test]
    fn mask_cell_preserves_casing_pattern() {
        assert_eq!(mask_cell("Seattle"), "Xxxxxxx");
        assert_eq!(mask_cell("PORTLAND"), "XXXXXXXX");
        assert_eq!(mask_cell("portland"), "xxxxxxxx");
    }

    #[test]
    fn mask_cell_preserves_whitespace() {
        assert_eq!(mask_cell("  Seattle "), "  Xxxxxxx ");
        assert_eq!(mask_cell("  "), "  ");
        assert_eq!(mask_cell(""), "");
    }

    #[test]
    fn mask_cell_preserves_number_format() {
        assert_eq!(mask_cell("$1,200.00"), "$9,999.99");
        assert_eq!(mask_cell("123.45"), "999.99");
        assert_eq!(mask_cell("(555) 867-5309"), "(999) 999-9999");
    }

    #[test]
    fn mask_cell_preserves_null_sentinels() {
        assert_eq!(mask_cell("N/A"), "N/A");
        assert_eq!(mask_cell("null"), "null");
        assert_eq!(mask_cell("NULL"), "NULL");
        assert_eq!(mask_cell("none"), "none");
        assert_eq!(mask_cell("None"), "None");
        assert_eq!(mask_cell("–"), "–");
        assert_eq!(mask_cell("-"), "-");
        assert_eq!(mask_cell("n/a"), "n/a");
    }

    #[test]
    fn mask_cell_preserves_trimmed_null_sentinels() {
        assert_eq!(mask_cell("  N/A  "), "  N/A  ");
        assert_eq!(mask_cell(" null "), " null ");
    }

    #[test]
    fn mask_sample_rows_masks_all_cells() {
        let input = "1,Seattle,WA\n2,portland,OR";
        let expected = "9,Xxxxxxx,XX\n9,xxxxxxxx,XX";
        assert_eq!(mask_sample_rows(input), expected);
    }

    #[test]
    fn mask_sample_rows_preserves_empty_lines() {
        let input = "hello,world\n\ngoodbye,world";
        let expected = "xxxxx,xxxxx\n\nxxxxxxx,xxxxx";
        assert_eq!(mask_sample_rows(input), expected);
    }
}
