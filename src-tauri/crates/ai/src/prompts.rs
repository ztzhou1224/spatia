use spatia_engine::TableColumn;

use std::collections::HashMap;

/// Type alias for column sample values: column_name → list of distinct values.
pub type ColumnSamples = HashMap<String, Vec<String>>;

/// Format the schema section with optional sample values for each column.
fn format_schema_with_samples(
    table_schemas: &[(String, Vec<TableColumn>)],
    all_samples: Option<&HashMap<String, ColumnSamples>>,
) -> String {
    let mut section = String::new();
    for (table_name, schema) in table_schemas {
        section.push_str(&format!("\n### Table: {}\n", table_name));
        let table_samples = all_samples.and_then(|s| s.get(table_name));
        for col in schema {
            section.push_str(&format!(
                "  - \"{}\" {} (not_null: {}, primary_key: {})",
                col.name, col.data_type, col.notnull, col.primary_key
            ));
            if let Some(samples) = table_samples.and_then(|s| s.get(&col.name)) {
                if !samples.is_empty() {
                    let quoted: Vec<String> = samples.iter().map(|v| format!("\"{}\"", v)).collect();
                    section.push_str(&format!(" — sample values: {}", quoted.join(", ")));
                }
            }
            section.push('\n');
        }
    }
    section
}

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
                "  \"{}\" {} (not_null: {})",
                col.name, col.data_type, col.notnull
            )
        })
        .collect();

    format!(
        r#"You are an expert data-cleaning assistant.
Your task is to write DuckDB SQL UPDATE statements that fix data quality issues
in the table `{table}`.

IMPORTANT: You are generating SQL for DuckDB, NOT PostgreSQL or MySQL.

## CRITICAL: Always double-quote column identifiers
Every column reference in your SQL MUST be wrapped in double quotes.
Example: UPDATE {table} SET "city" = TRIM("city") WHERE "city" IS NOT NULL;
This prevents errors when column names conflict with DuckDB reserved words or contain special characters.

## DuckDB string functions you may use
- UPPER(col), LOWER(col), TRIM(col), LTRIM(col), RTRIM(col)
- REPLACE(col, 'old', 'new')
- REGEXP_REPLACE(col, 'pattern', 'replacement')
- CONCAT(str1, str2, ...), LEFT(col, n), RIGHT(col, n), SUBSTRING(col, start, length)
- ILIKE (case-insensitive LIKE), REGEXP_MATCHES(col, 'pattern')
- CASE WHEN ... THEN ... ELSE ... END, COALESCE(col, default), NULLIF(col, value)

## DuckDB list/array functions you may use
- STRING_SPLIT(str, delimiter) — splits a string into a list
- LIST_TRANSFORM(list, x -> expr) — apply a lambda to every element
- ARRAY_TO_STRING(list, delimiter) — join a list back into a string
- LIST_FILTER(list, x -> condition) — filter list elements

## DuckDB regex dialect
- DuckDB uses RE2 syntax. Word boundary is `\b`, NOT `\m` (which is PostgreSQL-only).
- Example: REGEXP_REPLACE(col, '\bfoo\b', 'bar') — correct DuckDB word-boundary syntax.

## Common recipes

### Title-case a multi-word column (e.g. city names) — copy this exactly:
UPDATE {table} SET "col" = ARRAY_TO_STRING(
    LIST_TRANSFORM(
        STRING_SPLIT(LOWER(TRIM("col")), ' '),
        x -> CONCAT(UPPER(x[1]), x[2:])
    ), ' ')
WHERE "col" IS NOT NULL;

## DO NOT USE — these do not exist in DuckDB
- INITCAP — not available; use the LIST_TRANSFORM recipe above for title case
- STRING_SPLIT_BY_REGEX — does not exist; use STRING_SPLIT or REGEXP_REPLACE
- ARRAY_JOIN — does not exist; use ARRAY_TO_STRING instead
- \m in regex patterns — PostgreSQL-only word boundary; use \b in DuckDB/RE2

## DuckDB type casting
- CAST(col AS INTEGER), CAST(col AS DATE)
- TRY_CAST(col AS INTEGER) — prefer this for cleaning; returns NULL on failure instead of erroring

## DuckDB date functions
- STRFTIME(col, '%Y-%m-%d'), STRPTIME(str, '%Y-%m-%d'), CAST(col AS DATE)

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
2. Write minimal, targeted DuckDB `UPDATE {table} SET "col" = ... WHERE "col" IS NOT NULL` statements
   that fix each identified problem. ALWAYS double-quote every column name.
3. Return ONLY valid DuckDB SQL statements, one per line.
   Do NOT include explanations, markdown fences, or any other text.
   If no cleaning is needed, return the single comment: -- no changes needed
"#,
        table = table_name,
        schema = schema_lines.join("\n"),
        rows = sample_rows,
    )
}

/// Build a retry prompt that feeds a failed analysis SQL statement and its
/// DuckDB error back to the AI, asking for a corrected CREATE VIEW statement.
///
/// `user_question` — the original natural-language question.
/// `table_schemas` — same schema context used in the original prompt.
/// `failed_sql`    — the SQL that DuckDB rejected.
/// `error_message` — the raw DuckDB error string.
pub fn build_analysis_retry_prompt(
    user_question: &str,
    table_schemas: &[(String, Vec<TableColumn>)],
    failed_sql: &str,
    error_message: &str,
) -> String {
    build_analysis_retry_prompt_with_samples(
        user_question,
        table_schemas,
        failed_sql,
        error_message,
        None,
    )
}

/// Retry prompt with optional column sample values and domain context.
pub fn build_analysis_retry_prompt_with_samples(
    user_question: &str,
    table_schemas: &[(String, Vec<TableColumn>)],
    failed_sql: &str,
    error_message: &str,
    column_samples: Option<&HashMap<String, ColumnSamples>>,
) -> String {
    build_analysis_retry_prompt_with_domain(
        user_question,
        table_schemas,
        failed_sql,
        error_message,
        column_samples,
        None,
    )
}

/// Retry prompt with optional column sample values and domain context.
pub fn build_analysis_retry_prompt_with_domain(
    user_question: &str,
    table_schemas: &[(String, Vec<TableColumn>)],
    failed_sql: &str,
    error_message: &str,
    column_samples: Option<&HashMap<String, ColumnSamples>>,
    domain_context: Option<&str>,
) -> String {
    let schema_section = format_schema_with_samples(table_schemas, column_samples);

    let domain_section = match domain_context {
        Some(ctx) if !ctx.is_empty() => format!("\n{}\n", ctx),
        _ => String::new(),
    };

    format!(
        r#"The following DuckDB SQL failed to execute. Fix it and return a corrected version.
{domain}
## User question
{question}

## Available tables
{schemas}

## Failed SQL
{sql}

## DuckDB error
{error}

## Requirements
1. Return exactly one DuckDB SQL statement.
2. The SQL must be `CREATE OR REPLACE VIEW analysis_result AS ...`.
3. Use only columns present in the schemas above.
4. ALWAYS double-quote every column name (e.g. SELECT "city", "latitude" AS _lat). Unquoted column names cause "not found in FROM clause" errors.
5. Return ONLY the corrected SQL — no markdown, no explanation.
6. DO NOT use H3 functions (h3_latlng_to_cell, h3_cell_to_latlng, etc.) — they do not exist in DuckDB.
7. DO NOT use ST_HexagonGrid, ST_SquareGrid, or ST_MakeEnvelope — they do not exist in DuckDB spatial.
8. DO NOT use ST_Distance or ST_DWithin for distance calculations — DuckDB spatial does not support these on raw lat/lon pairs. Use Haversine formula instead.
9. For heatmap/hexbin visualizations, just return rows with lat/lon columns — the frontend handles spatial aggregation.
10. When filtering on categorical/enum columns, use the EXACT values shown in the sample values above. Do not guess or expand abbreviations.
"#,
        domain = domain_section,
        question = user_question.trim(),
        schemas = schema_section,
        sql = failed_sql,
        error = error_message,
    )
}

/// Build a single batch retry prompt for multiple failed statements.
///
/// Instead of sending N individual retry requests (one per failure), this
/// batches all failures into a single prompt. This reduces API round-trips
/// from N to 1, saving 3–8 seconds per avoided call.
pub fn build_clean_batch_retry_prompt(failures: &[(String, String)]) -> String {
    let mut failure_section = String::new();
    for (i, (sql, error)) in failures.iter().enumerate() {
        failure_section.push_str(&format!(
            "### Statement {n}\nSQL: {sql}\nError: {error}\n\n",
            n = i + 1,
            sql = sql,
            error = error,
        ));
    }

    format!(
        r#"The following DuckDB UPDATE statements failed with errors. Fix each one.

{failures}
## Rules
- Return one corrected UPDATE statement per failed statement above, in the same order.
- Each statement on its own line, ending with a semicolon.
- No markdown fences, no explanations, no numbering.
- ALWAYS double-quote every column name (e.g. "city", "wind_deductible_pct").
- DO NOT use: INITCAP, STRING_SPLIT_BY_REGEX, ARRAY_JOIN, or \m regex (PostgreSQL-only).
- For multi-word title case use this exact pattern:
    ARRAY_TO_STRING(LIST_TRANSFORM(STRING_SPLIT(LOWER(TRIM("col")), ' '), x -> CONCAT(UPPER(x[1]), x[2:])), ' ')
- For regex word boundaries use \b (RE2), NOT \m.
- Use ARRAY_TO_STRING (not ARRAY_JOIN) to join lists.
- Use STRING_SPLIT (not STRING_SPLIT_BY_REGEX) to split strings.
- Use TRY_CAST instead of CAST when converting types to avoid errors on bad data.
"#,
        failures = failure_section,
    )
}

/// Build a retry prompt that feeds a failed DuckDB statement and its error back
/// to the AI, asking for a corrected version.
#[cfg(test)]
pub fn build_clean_retry_prompt(failed_sql: &str, error_message: &str) -> String {
    format!(
        r#"The following DuckDB UPDATE statement failed with an error.

## Failed SQL
{sql}

## Error
{error}

Fix the SQL so it works correctly in DuckDB (NOT PostgreSQL or MySQL).

Rules:
- Return ONLY the corrected UPDATE statement, nothing else.
- No markdown fences, no explanations.
- ALWAYS double-quote every column name (e.g. "city", "wind_deductible_pct"). Unquoted column names cause "not found in FROM clause" errors.
- DO NOT use: INITCAP, STRING_SPLIT_BY_REGEX, ARRAY_JOIN, or \m regex (PostgreSQL-only).
- For multi-word title case use this exact pattern:
    ARRAY_TO_STRING(LIST_TRANSFORM(STRING_SPLIT(LOWER(TRIM("col")), ' '), x -> CONCAT(UPPER(x[1]), x[2:])), ' ')
- For regex word boundaries use \b (RE2), NOT \m.
- Use ARRAY_TO_STRING (not ARRAY_JOIN) to join lists.
- Use STRING_SPLIT (not STRING_SPLIT_BY_REGEX) to split strings.
- Use TRY_CAST instead of CAST when converting types to avoid errors on bad data.
"#,
        sql = failed_sql,
        error = error_message,
    )
}

/// Build a system prompt for analysis chat that injects the current DuckDB
/// schema as context before the user message is processed by the model.
pub fn build_analysis_chat_system_prompt(table_name: &str, schema: &[TableColumn]) -> String {
    build_analysis_chat_system_prompt_with_domain(table_name, schema, None)
}

/// Analysis chat system prompt with optional domain context.
pub fn build_analysis_chat_system_prompt_with_domain(
    table_name: &str,
    schema: &[TableColumn],
    domain_context: Option<&str>,
) -> String {
    let schema_lines: Vec<String> = schema
        .iter()
        .map(|col| {
            format!(
                "  - \"{}\" {} (not_null: {}, primary_key: {})",
                col.name, col.data_type, col.notnull, col.primary_key
            )
        })
        .collect();

    let domain_section = match domain_context {
        Some(ctx) if !ctx.is_empty() => format!("\n{}\n", ctx),
        _ => String::new(),
    };

    format!(
        r#"You are Spatia's SQL analysis assistant.
You are helping the user analyze geospatial data in DuckDB.
{domain}
## Active table
{table}

## Current schema
{schema}

## Rules
1. Ground your response in the provided schema.
2. Prefer DuckDB SQL that can run as-is.
3. ALWAYS double-quote every column and table name in SQL (e.g. SELECT "city" FROM "my_table").
4. If a requested field does not exist, state that clearly and suggest an alternative.
5. Keep responses concise and action-oriented.
"#,
        domain = domain_section,
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
    build_analysis_sql_prompt_with_domain(table_name, schema, user_goal, None)
}

/// Analysis SQL prompt with optional domain context.
pub fn build_analysis_sql_prompt_with_domain(
    table_name: &str,
    schema: &[TableColumn],
    user_goal: &str,
    domain_context: Option<&str>,
) -> String {
    let schema_lines: Vec<String> = schema
        .iter()
        .map(|col| {
            format!(
                "  - \"{}\" {} (not_null: {}, primary_key: {})",
                col.name, col.data_type, col.notnull, col.primary_key
            )
        })
        .collect();

    let domain_section = match domain_context {
        Some(ctx) if !ctx.is_empty() => format!("\n{}\n", ctx),
        _ => String::new(),
    };

    format!(
        r#"You are Spatia's DuckDB analysis SQL assistant.
{domain}
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
4. ALWAYS double-quote every column name (e.g. SELECT "city", "latitude" AS _lat). Unquoted column names cause "not found in FROM clause" errors.
5. Do not include markdown, comments, or explanation text.
6. DO NOT use H3 functions (h3_latlng_to_cell, h3_cell_to_latlng, etc.) or ST_HexagonGrid — they do not exist in DuckDB.
7. For heatmap/hexbin visualizations, just SELECT rows with lat/lon — the frontend handles spatial aggregation.
"#,
        domain = domain_section,
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

/// Build a multi-table-aware, conversation-aware prompt that instructs the
/// model to return structured JSON suitable for the unified `chat_turn` command.
pub fn build_unified_chat_prompt(
    table_schemas: &[(String, Vec<TableColumn>)],
    user_message: &str,
    conversation_history: &[serde_json::Value],
) -> String {
    build_unified_chat_prompt_with_samples(table_schemas, user_message, conversation_history, None)
}

/// Unified chat prompt with optional column sample values.
pub fn build_unified_chat_prompt_with_samples(
    table_schemas: &[(String, Vec<TableColumn>)],
    user_message: &str,
    conversation_history: &[serde_json::Value],
    column_samples: Option<&HashMap<String, ColumnSamples>>,
) -> String {
    build_unified_chat_prompt_with_domain(
        table_schemas,
        user_message,
        conversation_history,
        column_samples,
        None,
    )
}

/// Unified chat prompt with optional column samples and domain context.
pub fn build_unified_chat_prompt_with_domain(
    table_schemas: &[(String, Vec<TableColumn>)],
    user_message: &str,
    conversation_history: &[serde_json::Value],
    column_samples: Option<&HashMap<String, ColumnSamples>>,
    domain_context: Option<&str>,
) -> String {
    let schema_section = format_schema_with_samples(table_schemas, column_samples);

    // Format conversation history (last 10 turns)
    let mut history_section = String::new();
    let recent: Vec<&serde_json::Value> = conversation_history
        .iter()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    for msg in recent {
        if let (Some(role), Some(content)) = (
            msg.get("role").and_then(|v| v.as_str()),
            msg.get("content").and_then(|v| v.as_str()),
        ) {
            history_section.push_str(&format!("{}: {}\n", role, content));
        }
    }

    let domain_section = match domain_context {
        Some(ctx) if !ctx.is_empty() => format!("\n{}\n", ctx),
        _ => String::new(),
    };

    format!(
        r#"You are Spatia's GIS analysis assistant. You help users analyze geospatial data stored in DuckDB tables.
{domain}
## Available tables
{schemas}

## Conversation history
{history}

## User message
{message}

## Instructions
1. Answer the user's question using the available table schemas.
2. If the question requires data analysis, generate a DuckDB SQL query.
3. SQL MUST end with: CREATE OR REPLACE VIEW analysis_result AS <your final query>
4. Use only columns that exist in the schemas above.
5. ALWAYS double-quote every column name (e.g. SELECT "city", "latitude" AS _lat FROM "my_table"). Unquoted column names cause "not found in FROM clause" errors.
6. For geocoded tables, use _lat and _lon columns for coordinates.
7. You can JOIN across tables if the user's question requires it.
8. Double-quote table names too (e.g. FROM "commercial_property_portfolio").
9. For map navigation, include appropriate map_actions.
10. For complex analyses, you may use up to 5 intermediate views named `_spatia_step_1`, `_spatia_step_2`, etc. Each must be `CREATE OR REPLACE VIEW _spatia_step_N AS ...`. The final statement must always be `CREATE OR REPLACE VIEW analysis_result AS ...`. Separate all statements with semicolons.

### When to use multi-step SQL
Use intermediate views when the analysis requires multiple logical stages. For example, to filter raw data, then aggregate, then rank:
```
CREATE OR REPLACE VIEW _spatia_step_1 AS SELECT ... FROM raw WHERE ...;
CREATE OR REPLACE VIEW _spatia_step_2 AS SELECT region, COUNT(*) AS cnt FROM _spatia_step_1 GROUP BY region;
CREATE OR REPLACE VIEW analysis_result AS SELECT *, RANK() OVER (ORDER BY cnt DESC) AS rank FROM _spatia_step_2
```
For a simple query that needs no intermediate steps, a single statement is fine.

## Response format
Return ONLY valid JSON in this exact shape (no markdown, no extra text):
{{
  "message": "your natural language answer",
  "sql": "CREATE OR REPLACE VIEW analysis_result AS ... (or empty string if no SQL needed; for multi-step use semicolons between statements)",
  "visualization_type": "scatter",
  "map_actions": [
    {{"type": "fly_to", "center": [lng, lat], "zoom": 12}},
    {{"type": "fit_bounds", "bounds": [[west, south], [east, north]]}},
    {{"type": "show_popup", "coordinates": [lng, lat], "text": "..."}},
    {{"type": "highlight_features", "ids": ["id1", "id2"]}}
  ]
}}

## visualization_type selection

Types and when to use them:
- "scatter": individual points on the map — use when result has lat/lon or _lat/_lon columns
- "heatmap": density visualization for many overlapping points — use when coordinates are present and density patterns matter
- "hexbin": aggregated spatial grid — use when coordinates are present and you want concentration patterns
- "table": tabular results with no geographic component, or when exact values matter most
- "bar_chart": category-count aggregations, top-N rankings, group comparisons — requires one category column + one numeric column
- "pie_chart": proportion/share data, percentage breakdowns — requires one category + one numeric column, best with ≤8 categories
- "histogram": distribution of a single numeric variable (e.g. price distribution, age distribution)

Selection rules (apply in order):
1. Result has lat/lon or _lat/_lon columns → prefer scatter, heatmap, or hexbin
2. Query is a GROUP BY aggregation → prefer bar_chart
3. Query asks about proportions, shares, or percentages → prefer pie_chart
4. Query asks about distribution of a numeric value → prefer histogram
5. None of the above → use table

## CRITICAL: How map visualization types work

scatter, heatmap, and hexbin are ALL rendered by the FRONTEND (deck.gl).
The backend SQL must ONLY return individual rows with _lat and _lon (or lat/lon) columns.
The frontend handles all spatial aggregation, density calculation, and hex grid binning.

For scatter: SQL returns individual points → frontend plots them as dots.
For heatmap: SQL returns individual points → frontend renders a density heatmap.
For hexbin: SQL returns individual points → frontend aggregates into hex grid cells.

YOUR SQL MUST NEVER attempt spatial aggregation for these types. Just SELECT the relevant rows with their coordinates. You may filter, sort, or add computed columns, but do NOT try to bin, grid, or aggregate coordinates in SQL.

## DuckDB function blocklist — DO NOT USE these

The following functions DO NOT EXIST in DuckDB and will cause errors:
- h3_latlng_to_cell, h3_cell_to_latlng, h3_cell_to_boundary — H3 is not available
- ST_HexagonGrid, ST_SquareGrid, ST_MakeEnvelope — not available in DuckDB spatial
- ST_Distance, ST_DWithin — do not work on raw lat/lon pairs; use Haversine formula for distance calculations
- Any H3 or hex-grid function — these do not exist; the frontend handles hex binning

## CRITICAL: Column value matching
When filtering on categorical or enum columns, you MUST use the EXACT values shown in the "sample values" annotations in the schema above. Do NOT guess full-text labels for abbreviated values or vice versa. If no sample values are shown, query the data first or use ILIKE for flexible matching.

Only include map_actions when relevant. sql can be empty string if no query is needed.
"#,
        domain = domain_section,
        schemas = schema_section,
        history = if history_section.is_empty() {
            "(none)".to_string()
        } else {
            history_section
        },
        message = user_message.trim(),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_analysis_chat_system_prompt, build_analysis_retry_prompt, build_analysis_sql_prompt,
        build_clean_prompt, build_clean_retry_prompt, build_visualization_command_prompt,
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

    #[test]
    fn clean_prompt_contains_duckdb_dialect_guidance() {
        let prompt = build_clean_prompt("my_table", &sample_schema(), "");
        assert!(prompt.contains("DuckDB"));
        assert!(prompt.contains("INITCAP"));
        assert!(prompt.contains("TRY_CAST"));
        assert!(prompt.contains("REGEXP_REPLACE"));
    }

    #[test]
    fn retry_prompt_contains_failed_sql_and_error() {
        let sql = "UPDATE t SET city = INITCAP(TRIM(city)) WHERE city IS NOT NULL;";
        let err = "Scalar Function with name initcap does not exist!";
        let prompt = build_clean_retry_prompt(sql, err);
        assert!(prompt.contains(sql));
        assert!(prompt.contains(err));
        assert!(prompt.contains("DuckDB"));
        assert!(prompt.contains("INITCAP"));
    }

    #[test]
    fn analysis_retry_prompt_contains_question_sql_error_and_schema() {
        let schemas = vec![("places".to_string(), sample_schema())];
        let failed_sql =
            "CREATE OR REPLACE VIEW analysis_result AS SELECT nonexistent_col FROM places";
        let error = "Referenced column \"nonexistent_col\" not found in FROM clause";
        let prompt =
            build_analysis_retry_prompt("show all cities", &schemas, failed_sql, error);
        assert!(prompt.contains("show all cities"), "should include user question");
        assert!(prompt.contains(failed_sql), "should include the failed SQL");
        assert!(prompt.contains(error), "should include the DuckDB error");
        assert!(prompt.contains("places"), "should include the table name");
        assert!(prompt.contains("id"), "should include column names from schema");
        assert!(
            prompt.contains("CREATE OR REPLACE VIEW analysis_result AS"),
            "must instruct the model to use the required prefix"
        );
    }

    #[test]
    fn unified_chat_prompt_blocks_h3_and_explains_frontend_rendering() {
        let schemas = vec![("locations".to_string(), sample_schema())];
        let prompt = super::build_unified_chat_prompt(&schemas, "show hex grid", &[]);
        assert!(
            prompt.contains("h3_latlng_to_cell"),
            "should explicitly block H3 functions"
        );
        assert!(
            prompt.contains("frontend handles"),
            "should explain that frontend handles spatial aggregation"
        );
        assert!(
            prompt.contains("MUST NEVER attempt spatial aggregation"),
            "should forbid SQL-side spatial aggregation for map viz types"
        );
    }

    #[test]
    fn analysis_retry_prompt_blocks_h3_functions() {
        let schemas = vec![("places".to_string(), sample_schema())];
        let prompt = build_analysis_retry_prompt(
            "aggregate into hex grid",
            &schemas,
            "CREATE OR REPLACE VIEW analysis_result AS SELECT h3_latlng_to_cell(lat, lon, 7) FROM places",
            "Scalar Function with name h3_latlng_to_cell does not exist",
        );
        assert!(
            prompt.contains("h3_latlng_to_cell"),
            "retry prompt should block H3 functions"
        );
        assert!(
            prompt.contains("frontend handles spatial aggregation"),
            "retry prompt should explain frontend handles aggregation"
        );
    }

    #[test]
    fn analysis_sql_prompt_blocks_h3_functions() {
        let prompt = build_analysis_sql_prompt(
            "places",
            &sample_schema(),
            "show locations as hex grid",
        );
        assert!(
            prompt.contains("h3_latlng_to_cell"),
            "single-table prompt should block H3 functions"
        );
    }

    // --- Column quoting tests ---

    #[test]
    fn clean_prompt_schema_columns_are_double_quoted() {
        let prompt = build_clean_prompt("raw_staging", &sample_schema(), "");
        // Schema section must render column names wrapped in double quotes.
        assert!(
            prompt.contains("\"id\""),
            "clean prompt schema must double-quote the 'id' column"
        );
        assert!(
            prompt.contains("\"city\""),
            "clean prompt schema must double-quote the 'city' column"
        );
    }

    #[test]
    fn clean_prompt_includes_double_quote_instruction() {
        let prompt = build_clean_prompt("raw_staging", &sample_schema(), "");
        assert!(
            prompt.contains("double-quote") || prompt.contains("double quote"),
            "clean prompt must instruct the model to double-quote column identifiers"
        );
    }

    #[test]
    fn clean_prompt_recipe_uses_quoted_col_placeholder() {
        // The title-case recipe must show "col" in double quotes so the AI
        // learns the expected quoting convention from the example.
        let prompt = build_clean_prompt("raw_staging", &sample_schema(), "");
        assert!(
            prompt.contains("\"col\""),
            "clean prompt recipe must use quoted column placeholder (\"col\")"
        );
    }

    #[test]
    fn clean_retry_prompt_includes_double_quote_instruction() {
        let prompt = build_clean_retry_prompt(
            "UPDATE t SET city = TRIM(city)",
            "some error",
        );
        assert!(
            prompt.contains("double-quote") || prompt.contains("double quote"),
            "clean retry prompt must instruct the model to double-quote column identifiers"
        );
    }

    #[test]
    fn analysis_sql_prompt_schema_columns_are_double_quoted() {
        let prompt =
            build_analysis_sql_prompt("places", &sample_schema(), "count by city");
        assert!(
            prompt.contains("\"id\""),
            "analysis SQL prompt schema must double-quote the 'id' column"
        );
        assert!(
            prompt.contains("\"city\""),
            "analysis SQL prompt schema must double-quote the 'city' column"
        );
    }

    #[test]
    fn analysis_sql_prompt_includes_double_quote_instruction() {
        let prompt =
            build_analysis_sql_prompt("places", &sample_schema(), "count by city");
        assert!(
            prompt.contains("double-quote") || prompt.contains("double quote"),
            "analysis SQL prompt must instruct the model to double-quote column identifiers"
        );
    }

    #[test]
    fn analysis_chat_system_prompt_schema_columns_are_double_quoted() {
        let prompt = build_analysis_chat_system_prompt("places", &sample_schema());
        assert!(
            prompt.contains("\"id\""),
            "analysis chat system prompt schema must double-quote the 'id' column"
        );
        assert!(
            prompt.contains("\"city\""),
            "analysis chat system prompt schema must double-quote the 'city' column"
        );
    }

    #[test]
    fn analysis_chat_system_prompt_includes_double_quote_instruction() {
        let prompt = build_analysis_chat_system_prompt("places", &sample_schema());
        assert!(
            prompt.contains("double-quote") || prompt.contains("double quote"),
            "analysis chat system prompt must instruct the model to double-quote column identifiers"
        );
    }

    #[test]
    fn analysis_retry_prompt_includes_double_quote_instruction() {
        let schemas = vec![("places".to_string(), sample_schema())];
        let prompt = build_analysis_retry_prompt(
            "show all cities",
            &schemas,
            "CREATE OR REPLACE VIEW analysis_result AS SELECT city FROM places",
            "some error",
        );
        assert!(
            prompt.contains("double-quote") || prompt.contains("double quote"),
            "analysis retry prompt must instruct the model to double-quote column identifiers"
        );
    }

    #[test]
    fn unified_chat_prompt_schema_columns_are_double_quoted() {
        let schemas = vec![("locations".to_string(), sample_schema())];
        let prompt = super::build_unified_chat_prompt(&schemas, "show cities", &[]);
        assert!(
            prompt.contains("\"id\""),
            "unified chat prompt schema must double-quote the 'id' column"
        );
        assert!(
            prompt.contains("\"city\""),
            "unified chat prompt schema must double-quote the 'city' column"
        );
    }

    #[test]
    fn unified_chat_prompt_includes_double_quote_instruction() {
        let schemas = vec![("locations".to_string(), sample_schema())];
        let prompt = super::build_unified_chat_prompt(&schemas, "show cities", &[]);
        assert!(
            prompt.contains("double-quote") || prompt.contains("double quote"),
            "unified chat prompt must instruct the model to double-quote column identifiers"
        );
    }
}
