use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tracing::{debug, error, info, warn};

use spatia_ai::{build_analysis_retry_prompt, build_unified_chat_prompt, GeminiClient};
use spatia_engine::{execute_analysis_sql_to_geojson, ingest_csv_to_table, table_schema};

use crate::corpus::TestCase;

const MAX_RETRIES: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestOutcome {
    Pass,
    AssertionFailure(String),
    InvalidSql(String),
    AiResponseError(String),
    ApiError(String),
    Timeout,
    SetupError(String),
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TimingMs {
    pub total_ms: u64,
    pub ai_ms: u64,
    pub sql_ms: u64,
    pub setup_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TestResult {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub outcome: String,
    pub outcome_detail: Option<String>,
    pub sql_generated: Option<String>,
    pub round_trips: usize,
    pub timing: TimingMs,
    pub result_row_count: Option<usize>,
    pub result_columns: Option<Vec<String>>,
}

impl TestResult {
    fn from_outcome(tc: &TestCase, outcome: TestOutcome, detail: RunDetail) -> Self {
        let (outcome_str, outcome_detail) = match &outcome {
            TestOutcome::Pass => ("pass".to_string(), None),
            TestOutcome::AssertionFailure(msg) => ("assertion_failure".to_string(), Some(msg.clone())),
            TestOutcome::InvalidSql(msg) => ("invalid_sql".to_string(), Some(msg.clone())),
            TestOutcome::AiResponseError(msg) => ("ai_response_error".to_string(), Some(msg.clone())),
            TestOutcome::ApiError(msg) => ("api_error".to_string(), Some(msg.clone())),
            TestOutcome::Timeout => ("timeout".to_string(), None),
            TestOutcome::SetupError(msg) => ("setup_error".to_string(), Some(msg.clone())),
        };
        TestResult {
            name: tc.name.clone(),
            description: tc.description.clone(),
            tags: tc.tags.clone(),
            outcome: outcome_str,
            outcome_detail,
            sql_generated: detail.sql_generated,
            round_trips: detail.round_trips,
            timing: detail.timing,
            result_row_count: detail.result_row_count,
            result_columns: detail.result_columns,
        }
    }
}

#[derive(Default)]
struct RunDetail {
    sql_generated: Option<String>,
    round_trips: usize,
    timing: TimingMs,
    result_row_count: Option<usize>,
    result_columns: Option<Vec<String>>,
}

pub struct RunnerContext {
    pub client: GeminiClient,
    pub corpus_dir: PathBuf,
    pub default_timeout_secs: u64,
}

pub async fn run_test(ctx: &RunnerContext, tc: &TestCase) -> TestResult {
    let test_start = Instant::now();
    let mut detail = RunDetail::default();

    let csv_path = ctx.corpus_dir.join(&tc.setup_csv);
    let csv_path = csv_path.canonicalize().unwrap_or(csv_path);
    let csv_path_str = csv_path.to_string_lossy().to_string();

    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let db_path = format!("/tmp/spatia_bench_test_{ns}.duckdb");

    info!(test = %tc.name, db = %db_path, "run_test: starting");

    // Setup: ingest CSV
    let setup_start = Instant::now();
    if let Err(e) = ingest_csv_to_table(&db_path, &csv_path_str, &tc.setup_table) {
        let msg = format!("CSV ingest failed for '{}': {}", csv_path_str, e);
        error!(test = %tc.name, error = %msg);
        detail.timing.setup_ms = setup_start.elapsed().as_millis() as u64;
        detail.timing.total_ms = test_start.elapsed().as_millis() as u64;
        cleanup_db(&db_path);
        return TestResult::from_outcome(tc, TestOutcome::SetupError(msg), detail);
    }
    detail.timing.setup_ms = setup_start.elapsed().as_millis() as u64;

    // Fetch schema
    let schema = match table_schema(&db_path, &tc.setup_table) {
        Ok(s) => s,
        Err(e) => {
            cleanup_db(&db_path);
            detail.timing.total_ms = test_start.elapsed().as_millis() as u64;
            return TestResult::from_outcome(
                tc,
                TestOutcome::SetupError(format!("schema fetch failed: {e}")),
                detail,
            );
        }
    };
    let table_schemas = vec![(tc.setup_table.clone(), schema)];

    // AI + SQL loop with retry
    let timeout = std::time::Duration::from_secs(
        tc.timeout_secs.unwrap_or(ctx.default_timeout_secs),
    );
    let outcome = tokio::time::timeout(
        timeout,
        ai_sql_loop(ctx, tc, &db_path, &table_schemas, &mut detail),
    )
    .await
    .unwrap_or_else(|_| {
        warn!(test = %tc.name, "run_test: timed out");
        TestOutcome::Timeout
    });

    detail.timing.total_ms = test_start.elapsed().as_millis() as u64;
    cleanup_db(&db_path);

    TestResult::from_outcome(tc, outcome, detail)
}

async fn ai_sql_loop(
    ctx: &RunnerContext,
    tc: &TestCase,
    db_path: &str,
    table_schemas: &[(String, Vec<spatia_engine::TableColumn>)],
    detail: &mut RunDetail,
) -> TestOutcome {
    let mut last_sql: Option<String> = None;
    let mut last_error: Option<String> = None;

    for attempt in 0..=MAX_RETRIES {
        detail.round_trips = attempt + 1;

        let prompt = if attempt == 0 {
            build_unified_chat_prompt(table_schemas, &tc.query, &[])
        } else {
            build_analysis_retry_prompt(
                &tc.query,
                table_schemas,
                last_sql.as_deref().unwrap_or(""),
                last_error.as_deref().unwrap_or("unknown error"),
            )
        };

        // Call Gemini
        let ai_start = Instant::now();
        let ai_response = if attempt == 0 {
            ctx.client.generate_json(&prompt).await
        } else {
            ctx.client.generate(&prompt).await
        };
        detail.timing.ai_ms += ai_start.elapsed().as_millis() as u64;

        let raw_response = match ai_response {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("429") || msg.contains("quota") {
                    warn!(test = %tc.name, attempt, "rate limited, backing off");
                    tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt as u32 + 1))).await;
                    last_error = Some(msg);
                    continue;
                }
                return TestOutcome::ApiError(msg);
            }
        };

        // Extract SQL
        let sql = if attempt == 0 {
            extract_sql_from_json(&raw_response)
        } else {
            Some(strip_markdown_fences(&raw_response))
        };

        let sql = match sql {
            Some(s) if !s.is_empty() => s,
            _ => {
                if !tc.expect_success {
                    return TestOutcome::Pass;
                }
                return TestOutcome::AiResponseError("AI returned no SQL".to_string());
            }
        };

        debug!(test = %tc.name, attempt, sql = %sql, "executing SQL");
        last_sql = Some(sql.clone());
        detail.sql_generated = Some(sql.clone());

        // Execute SQL
        let sql_start = Instant::now();
        let exec_result = execute_analysis_sql_to_geojson(db_path, &sql);
        detail.timing.sql_ms += sql_start.elapsed().as_millis() as u64;

        match exec_result {
            Ok(result) => {
                detail.result_row_count = Some(result.row_count);
                detail.result_columns = Some(result.tabular.columns.clone());
                return run_assertions(tc, &sql, &result);
            }
            Err(e) => {
                let err_msg = e.to_string();
                warn!(test = %tc.name, attempt, error = %err_msg, "SQL failed");
                last_error = Some(err_msg.clone());

                if attempt == MAX_RETRIES {
                    return if tc.expect_success {
                        TestOutcome::InvalidSql(err_msg)
                    } else {
                        TestOutcome::Pass
                    };
                }
            }
        }
    }

    TestOutcome::InvalidSql(last_error.unwrap_or_else(|| "unknown".to_string()))
}

fn extract_sql_from_json(raw: &str) -> Option<String> {
    let value: Value = serde_json::from_str(raw).ok()?;
    let sql = value.get("sql")?.as_str()?;
    if sql.trim().is_empty() {
        None
    } else {
        Some(sql.trim().to_string())
    }
}

fn strip_markdown_fences(text: &str) -> String {
    let trimmed = text.trim();
    let without_start = trimmed
        .strip_prefix("```sql")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let without_end = without_start
        .strip_suffix("```")
        .unwrap_or(without_start);
    without_end.trim().to_string()
}

fn run_assertions(
    tc: &TestCase,
    sql: &str,
    result: &spatia_engine::AnalysisExecutionResult,
) -> TestOutcome {
    let sql_upper = sql.to_uppercase();

    for fragment in &tc.expect_sql_contains {
        if !sql_upper.contains(&fragment.to_uppercase()) {
            return TestOutcome::AssertionFailure(format!(
                "expected SQL to contain '{fragment}'"
            ));
        }
    }

    for fragment in &tc.expect_sql_not_contains {
        if sql_upper.contains(&fragment.to_uppercase()) {
            return TestOutcome::AssertionFailure(format!(
                "expected SQL to NOT contain '{fragment}'"
            ));
        }
    }

    if let Some(expected) = tc.expect_row_count {
        if result.row_count != expected {
            return TestOutcome::AssertionFailure(format!(
                "expected {} row(s) but got {}",
                expected, result.row_count
            ));
        }
    }

    if let Some(min) = tc.expect_min_rows {
        if result.row_count < min {
            return TestOutcome::AssertionFailure(format!(
                "expected at least {} row(s) but got {}",
                min, result.row_count
            ));
        }
    }

    if !tc.expect_columns.is_empty() {
        let result_cols_upper: Vec<String> = result
            .tabular
            .columns
            .iter()
            .map(|c| c.to_uppercase())
            .collect();
        for expected_col in &tc.expect_columns {
            if !result_cols_upper.contains(&expected_col.to_uppercase()) {
                return TestOutcome::AssertionFailure(format!(
                    "expected column '{}' in result but got: {:?}",
                    expected_col, result.tabular.columns
                ));
            }
        }
    }

    TestOutcome::Pass
}

pub fn cleanup_db(path: &str) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}.wal"));
    let _ = std::fs::remove_file(format!("{path}.wal.lck"));
}
