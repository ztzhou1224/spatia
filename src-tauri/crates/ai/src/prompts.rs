use spatia_engine::TableColumn;

/// Build a system + user prompt that instructs the AI to return DuckDB `UPDATE`
/// statements which clean the data in `table_name`.
///
/// `schema`      — column metadata obtained via `spatia_engine::table_schema`.
/// `sample_rows` — a string containing sample rows (e.g., CSV or JSON lines)
///                  used to give the model concrete examples of the data.
pub fn build_clean_prompt(table_name: &str, schema: &[TableColumn], sample_rows: &str) -> String {
    let schema_lines: Vec<String> = schema
        .iter()
        .map(|col| {
            format!(
                "  {} {} (not_null: {})",
                col.name, col.data_type, col.notnull
            )
        })
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

/// Build a system prompt for analysis chat that injects the current DuckDB
/// schema as context before the user message is processed by the model.
pub fn build_analysis_chat_system_prompt(table_name: &str, schema: &[TableColumn]) -> String {
    let schema_lines: Vec<String> = schema
        .iter()
        .map(|col| {
            format!(
                "  - {} {} (not_null: {}, primary_key: {})",
                col.name, col.data_type, col.notnull, col.primary_key
            )
        })
        .collect();

    format!(
        r#"You are Spatia's SQL analysis assistant.
You are helping the user analyze geospatial data in DuckDB.

## Active table
{table}

## Current schema
{schema}

## Rules
1. Ground your response in the provided schema.
2. Prefer DuckDB SQL that can run as-is.
3. If a requested field does not exist, state that clearly and suggest an alternative.
4. Keep responses concise and action-oriented.
"#,
        table = table_name,
        schema = schema_lines.join("\n"),
    )
}

/// Build a prompt that asks the model for a single DuckDB SQL statement that
/// creates or replaces an `analysis_result` view from an input table schema.
pub fn build_analysis_sql_prompt(
    table_name: &str,
    schema: &[TableColumn],
    user_goal: &str,
) -> String {
    let schema_lines: Vec<String> = schema
        .iter()
        .map(|col| {
            format!(
                "  - {} {} (not_null: {}, primary_key: {})",
                col.name, col.data_type, col.notnull, col.primary_key
            )
        })
        .collect();

    format!(
        r#"You are Spatia's DuckDB analysis SQL assistant.

## Input table
{table}

## Current schema
{schema}

## User goal
{goal}

## Requirements
1. Return exactly one DuckDB SQL statement.
2. The SQL must be `CREATE OR REPLACE VIEW analysis_result AS ...`.
3. Use only columns present in the schema.
4. Do not include markdown, comments, or explanation text.
"#,
        table = table_name,
        schema = schema_lines.join("\n"),
        goal = user_goal.trim(),
    )
}

/// Build a prompt that requests a structured visualization command in JSON.
pub fn build_visualization_command_prompt(table_name: &str, user_goal: &str) -> String {
    format!(
        r#"You are Spatia's visualization planner.

## Input context
- table: {table}
- goal: {goal}

Return ONLY compact JSON in this exact shape:
{{"visualization":"scatter"}}

Allowed visualization values: scatter, heatmap, hexbin.
No markdown. No explanation.
"#,
        table = table_name,
        goal = user_goal.trim(),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_analysis_chat_system_prompt, build_analysis_sql_prompt, build_clean_prompt,
        build_visualization_command_prompt,
    };
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

    #[test]
    fn analysis_system_prompt_contains_schema_context() {
        let prompt = build_analysis_chat_system_prompt("places", &sample_schema());
        assert!(prompt.contains("Active table"));
        assert!(prompt.contains("places"));
        assert!(prompt.contains("id"));
        assert!(prompt.contains("city"));
    }

    #[test]
    fn analysis_sql_prompt_requires_analysis_result_view() {
        let prompt = build_analysis_sql_prompt(
            "places",
            &sample_schema(),
            "find top cities by record count",
        );
        assert!(prompt.contains("analysis_result"));
        assert!(prompt.contains("CREATE OR REPLACE VIEW"));
        assert!(prompt.contains("find top cities by record count"));
    }

    #[test]
    fn visualization_prompt_requires_json_shape() {
        let prompt = build_visualization_command_prompt("analysis_result", "show hotspots");
        assert!(prompt.contains("{\"visualization\":\"scatter\"}"));
        assert!(prompt.contains("scatter, heatmap, hexbin"));
    }
}
