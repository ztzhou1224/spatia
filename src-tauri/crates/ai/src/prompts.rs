use spatia_engine::TableColumn;

/// Build a system + user prompt that instructs the AI to return DuckDB `UPDATE`
/// statements which clean the data in `table_name`.
///
/// `schema`      — column metadata obtained via `spatia_engine::table_schema`.
/// `sample_rows` — a string containing sample rows (e.g., CSV or JSON lines)
///                  used to give the model concrete examples of the data.
pub fn build_clean_prompt(
    table_name: &str,
    schema: &[TableColumn],
    sample_rows: &str,
) -> String {
    let schema_lines: Vec<String> = schema
        .iter()
        .map(|col| format!("  {} {} (not_null: {})", col.name, col.data_type, col.notnull))
        .collect();

    format!(
        r#"You are an expert data-cleaning assistant.
Your task is to write DuckDB SQL UPDATE statements that fix data quality issues
in the table `{table}`.

## Table schema
{schema}

## Sample rows (up to 20)
{rows}

## Instructions
1. Inspect the sample rows for common data-quality problems such as:
   - Leading/trailing whitespace in text columns.
   - Inconsistent casing (e.g., mixed upper/lower case city names).
   - Obvious null sentinels stored as strings (e.g., "N/A", "null", "none", "–").
   - Numeric values stored as strings with extra punctuation (e.g., "$1,200.00").
2. Write minimal, targeted DuckDB `UPDATE {table} SET ... WHERE ...` statements
   that fix each identified problem.
3. Return ONLY valid DuckDB SQL statements, one per line.
   Do NOT include explanations, markdown fences, or any other text.
   If no cleaning is needed, return the single comment: -- no changes needed
"#,
        table = table_name,
        schema = schema_lines.join("\n"),
        rows = sample_rows,
    )
}

#[cfg(test)]
mod tests {
    use super::build_clean_prompt;
    use spatia_engine::TableColumn;

    fn sample_schema() -> Vec<TableColumn> {
        vec![
            TableColumn {
                cid: 0,
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                notnull: true,
                default_value: None,
                primary_key: true,
            },
            TableColumn {
                cid: 1,
                name: "city".to_string(),
                data_type: "VARCHAR".to_string(),
                notnull: false,
                default_value: None,
                primary_key: false,
            },
        ]
    }

    #[test]
    fn prompt_contains_table_name() {
        let prompt = build_clean_prompt("raw_staging", &sample_schema(), "1,Seattle\n2,portland");
        assert!(prompt.contains("raw_staging"));
    }

    #[test]
    fn prompt_contains_column_names() {
        let prompt = build_clean_prompt("raw_staging", &sample_schema(), "");
        assert!(prompt.contains("id"));
        assert!(prompt.contains("city"));
    }

    #[test]
    fn prompt_contains_sample_rows() {
        let rows = "1,Seattle\n2,portland";
        let prompt = build_clean_prompt("my_table", &sample_schema(), rows);
        assert!(prompt.contains("Seattle"));
        assert!(prompt.contains("portland"));
    }

    #[test]
    fn prompt_contains_update_instruction() {
        let prompt = build_clean_prompt("raw_staging", &sample_schema(), "");
        assert!(prompt.contains("UPDATE"));
    }
}
