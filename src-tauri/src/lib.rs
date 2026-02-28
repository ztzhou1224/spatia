// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

/// Resolved at startup by `run()` → `setup` hook via Tauri's app-data dir.
/// Falls back to the legacy relative path only if setup never ran (e.g. unit tests).
static DB_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn db_path() -> &'static str {
    DB_PATH
        .get()
        .map(String::as_str)
        .unwrap_or("src-tauri/spatia.duckdb")
}

#[derive(Debug, Clone, Serialize)]
struct AnalysisChatResponse {
    assistant: String,
    system_prompt: String,
}

#[derive(Debug, Clone, Serialize)]
struct AnalysisSqlResponse {
    sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VisualizationCommandResponse {
    visualization: String,
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn execute_engine_command(command: String) -> Result<String, String> {
    spatia_engine::execute_command(&command).map_err(|err| err.to_string())
}

#[derive(Debug, Clone, Serialize)]
struct IngestProgressEvent {
    stage: &'static str,
    message: String,
    percent: u8,
}

fn emit_ingest_progress(
    app: &tauri::AppHandle,
    stage: &'static str,
    message: impl Into<String>,
    percent: u8,
) -> Result<(), String> {
    app.emit(
        "ingest-progress",
        IngestProgressEvent {
            stage,
            message: message.into(),
            percent,
        },
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn ingest_csv_with_progress(
    app: tauri::AppHandle,
    csv_path: String,
    table_name: Option<String>,
) -> Result<String, String> {
    emit_ingest_progress(&app, "started", "Starting CSV ingestion", 5)?;
    emit_ingest_progress(&app, "reading", format!("Reading file: {csv_path}"), 30)?;

    let resolved_table = table_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());

    let ingest_result = if let Some(table) = resolved_table {
        spatia_engine::ingest_csv_to_table(db_path(), &csv_path, table)
            .map(|_| table.to_string())
            .map_err(|err| err.to_string())
    } else {
        spatia_engine::ingest_csv(db_path(), &csv_path)
            .map(|_| "raw_staging".to_string())
            .map_err(|err| err.to_string())
    };

    match ingest_result {
        Ok(table) => {
            emit_ingest_progress(&app, "writing", format!("Loaded table: {table}"), 85)?;
            emit_ingest_progress(&app, "completed", "Ingestion complete", 100)?;
            Ok(format!("{{\"status\":\"ok\",\"table\":\"{}\"}}", table))
        }
        Err(err) => {
            let _ = emit_ingest_progress(&app, "failed", format!("Ingestion failed: {err}"), 100);
            Err(err)
        }
    }
}

// ---- Clean progress ----

#[derive(Debug, Clone, Serialize)]
struct CleanProgressEvent {
    stage: &'static str,
    message: String,
    percent: u8,
    round: u8,
}

fn emit_clean_progress(
    app: &tauri::AppHandle,
    stage: &'static str,
    message: impl Into<String>,
    percent: u8,
    round: u8,
) -> Result<(), String> {
    app.emit(
        "clean-progress",
        CleanProgressEvent {
            stage,
            message: message.into(),
            percent,
            round,
        },
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
async fn clean_table_with_progress(
    app: tauri::AppHandle,
    table_name: String,
) -> Result<String, String> {
    let client = match spatia_ai::GeminiClient::from_env() {
        Ok(c) => c,
        Err(_) => {
            return Ok(r#"{"status":"skipped","reason":"no_api_key"}"#.to_string());
        }
    };

    const MAX_ROUNDS: u8 = 5;
    let mut total_statements = 0usize;
    let mut last_round = 0u8;

    for round in 1..=MAX_ROUNDS {
        last_round = round;
        let pct_start = ((round as u16 - 1) * 18 + 5).min(80) as u8;

        emit_clean_progress(
            &app,
            "round_start",
            format!("Cleaning round {round}..."),
            pct_start,
            round,
        )?;

        let result = spatia_ai::clean_table(db_path(), &table_name, &client)
            .await
            .map_err(|e| e.to_string())?;

        if result.statements_applied.is_empty() {
            emit_clean_progress(
                &app,
                "no_changes",
                format!("No changes needed in round {round}"),
                100,
                round,
            )?;
            break;
        }

        total_statements += result.statements_applied.len();
        let pct = ((round as u16 - 1) * 18 + 14).min(90) as u8;
        emit_clean_progress(
            &app,
            "round_applied",
            format!(
                "Round {round}: {} statement(s) applied",
                result.statements_applied.len()
            ),
            pct,
            round,
        )?;
    }

    emit_clean_progress(
        &app,
        "completed",
        format!(
            "{last_round} round(s), {total_statements} statement(s) applied"
        ),
        100,
        last_round,
    )?;

    let json = serde_json::json!({
        "status": "ok",
        "table": table_name,
        "rounds": last_round,
        "total_statements": total_statements,
    });
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

// ---- Detect address columns ----

#[tauri::command]
fn detect_address_columns(table_name: String) -> Result<String, String> {
    let schema =
        spatia_engine::table_schema(db_path(), &table_name).map_err(|e| e.to_string())?;

    let address_columns: Vec<String> = schema
        .into_iter()
        .filter(|col| {
            let name = col.name.to_lowercase();
            let dtype = col.data_type.to_lowercase();

            // Only VARCHAR/TEXT/STRING types
            if !dtype.contains("varchar") && !dtype.contains("text") && !dtype.contains("string") {
                return false;
            }

            // Exclude columns with these terms
            if ["ip", "email", "url", "web"]
                .iter()
                .any(|p| name.contains(p))
            {
                return false;
            }

            // Include if name contains address-related terms or is an exact match
            ["address", "addr", "street", "location", "place"]
                .iter()
                .any(|p| name.contains(p))
                || ["city", "zip", "postal", "suburb", "neighbourhood"]
                    .contains(&name.as_str())
        })
        .map(|col| col.name)
        .collect();

    let json = serde_json::json!({ "columns": address_columns });
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

// ---- Geocode progress ----

#[derive(Debug, Clone, Serialize)]
struct GeocodeProgressEvent {
    stage: &'static str,
    message: String,
    percent: u8,
}

fn emit_geocode_progress(
    app: &tauri::AppHandle,
    stage: &'static str,
    message: impl Into<String>,
    percent: u8,
) -> Result<(), String> {
    app.emit(
        "geocode-progress",
        GeocodeProgressEvent {
            stage,
            message: message.into(),
            percent,
        },
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
async fn geocode_table_column(
    app: tauri::AppHandle,
    table_name: String,
    address_col: String,
) -> Result<String, String> {
    spatia_engine::validate_table_name(&table_name).map_err(|e| e.to_string())?;

    // Column names must not contain double-quotes (which would break our quoting)
    if address_col.is_empty() || address_col.contains('"') {
        return Err("invalid address column name".to_string());
    }

    emit_geocode_progress(&app, "extracting", "Extracting unique addresses...", 0)?;

    // Extract distinct non-null addresses (block-scoped so the connection closes before geocode_batch)
    let addresses: Vec<String> = {
        let conn =
            duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;
        let sql = format!(
            r#"SELECT DISTINCT "{col}" FROM "{table}" WHERE "{col}" IS NOT NULL"#,
            col = address_col,
            table = table_name,
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut out: Vec<String> = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| e.to_string())?);
        }
        out
    };

    let total_addresses = addresses.len();
    emit_geocode_progress(
        &app,
        "geocoding",
        format!("Geocoding {total_addresses} unique address(es)..."),
        20,
    )?;

    // geocode_batch opens and closes its own connection
    let results =
        spatia_engine::geocode_batch(db_path(), &addresses).map_err(|e| e.to_string())?;
    let geocoded_count = results.len();

    emit_geocode_progress(
        &app,
        "geocoded",
        format!("Geocoded {geocoded_count}/{total_addresses} addresses"),
        70,
    )?;

    if !results.is_empty() {
        let conn =
            duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;

        // Add geocode columns if not already present
        for alter_sql in [
            format!(
                r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS _lat DOUBLE"#,
                table_name
            ),
            format!(
                r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS _lon DOUBLE"#,
                table_name
            ),
            format!(
                r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS _geocode_source VARCHAR"#,
                table_name
            ),
            format!(
                r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS _geocode_confidence DOUBLE"#,
                table_name
            ),
        ] {
            conn.execute_batch(&alter_sql).map_err(|e| e.to_string())?;
        }

        // Build VALUES list for a temp staging table
        let values: Vec<String> = results
            .iter()
            .map(|r| {
                format!(
                    "('{}', {}, {}, '{}', {})",
                    r.address.replace('\'', "''"),
                    r.lat,
                    r.lon,
                    r.source.replace('\'', "''"),
                    r.confidence,
                )
            })
            .collect();

        conn.execute_batch(&format!(
            "CREATE OR REPLACE TEMP TABLE _gc AS \
             SELECT * FROM (VALUES {}) AS t(address, lat, lon, source, confidence)",
            values.join(", "),
        ))
        .map_err(|e| e.to_string())?;

        conn.execute_batch(&format!(
            r#"UPDATE "{table}" SET _lat = g.lat, _lon = g.lon,
               _geocode_source = g.source, _geocode_confidence = g.confidence
               FROM _gc g WHERE "{table}"."{col}" = g.address"#,
            table = table_name,
            col = address_col,
        ))
        .map_err(|e| e.to_string())?;

        conn.execute_batch("DROP TABLE IF EXISTS _gc")
            .map_err(|e| e.to_string())?;
    }

    emit_geocode_progress(&app, "completed", "Geocoding complete", 100)?;

    let json = serde_json::json!({
        "status": "ok",
        "table": table_name,
        "geocoded_count": geocoded_count,
        "total_addresses": total_addresses,
    });
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

// ---- Drop table ----

#[tauri::command]
fn drop_table(table_name: String) -> Result<String, String> {
    spatia_engine::validate_table_name(&table_name).map_err(|e| e.to_string())?;

    let conn = duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;
    conn.execute_batch(&format!(
        r#"DROP TABLE IF EXISTS "{}""#,
        table_name
    ))
    .map_err(|e| e.to_string())?;

    let json = serde_json::json!({ "status": "ok", "table": table_name });
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

// ---- Analysis commands ----

#[tauri::command]
async fn analysis_chat(table_name: String, user_message: String) -> Result<String, String> {
    if user_message.trim().is_empty() {
        return Err("user_message cannot be empty".to_string());
    }

    let schema =
        spatia_engine::table_schema(db_path(), &table_name).map_err(|err| err.to_string())?;
    let system_prompt = spatia_ai::build_analysis_chat_system_prompt(&table_name, &schema);
    let full_prompt = format!(
        "{system}\n\n## User message\n{message}\n",
        system = system_prompt,
        message = user_message.trim()
    );

    let assistant = match spatia_ai::GeminiClient::from_env() {
        Ok(client) => client
            .generate(&full_prompt)
            .await
            .map_err(|err| err.to_string())?,
        Err(_) => "Gemini is not configured. Set SPATIA_GEMINI_API_KEY to enable AI analysis chat."
            .to_string(),
    };

    let payload = AnalysisChatResponse {
        assistant,
        system_prompt,
    };
    serde_json::to_string(&payload).map_err(|err| err.to_string())
}

#[tauri::command]
async fn generate_analysis_sql(table_name: String, user_goal: String) -> Result<String, String> {
    if user_goal.trim().is_empty() {
        return Err("user_goal cannot be empty".to_string());
    }

    let schema =
        spatia_engine::table_schema(db_path(), &table_name).map_err(|err| err.to_string())?;
    let prompt = spatia_ai::build_analysis_sql_prompt(&table_name, &schema, &user_goal);

    let sql = match spatia_ai::GeminiClient::from_env() {
        Ok(client) => client
            .generate(&prompt)
            .await
            .map_err(|err| err.to_string())?,
        Err(_) => {
            format!(
                "CREATE OR REPLACE VIEW analysis_result AS SELECT * FROM {} LIMIT 100;",
                table_name
            )
        }
    }
    .trim()
    .to_string();

    let payload = AnalysisSqlResponse { sql };
    serde_json::to_string(&payload).map_err(|err| err.to_string())
}

#[tauri::command]
fn execute_analysis_sql(sql: String) -> Result<String, String> {
    let result = spatia_engine::execute_analysis_sql_to_geojson(db_path(), &sql)
        .map_err(|err| err.to_string())?;
    serde_json::to_string(&result).map_err(|err| err.to_string())
}

#[tauri::command]
async fn generate_visualization_command(
    table_name: String,
    user_goal: String,
) -> Result<String, String> {
    if user_goal.trim().is_empty() {
        return Err("user_goal cannot be empty".to_string());
    }

    let prompt = spatia_ai::build_visualization_command_prompt(&table_name, &user_goal);

    let visualization = match spatia_ai::GeminiClient::from_env() {
        Ok(client) => {
            let text = client
                .generate(&prompt)
                .await
                .map_err(|err| err.to_string())?;
            match serde_json::from_str::<VisualizationCommandResponse>(&text) {
                Ok(parsed) => parsed.visualization,
                Err(_) => "scatter".to_string(),
            }
        }
        Err(_) => "scatter".to_string(),
    };

    serde_json::to_string(&VisualizationCommandResponse { visualization })
        .map_err(|err| err.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Resolve an absolute, stable path for the DuckDB file using the
            // platform app-data directory (e.g. ~/Library/Application Support/…).
            // This avoids ambiguity with relative paths whose resolution depends
            // on the process working directory, which varies between dev and prod.
            if let Ok(data_dir) = app.path().app_data_dir() {
                let _ = std::fs::create_dir_all(&data_dir);
                let db = data_dir.join("spatia.duckdb");
                let _ = DB_PATH.set(db.to_string_lossy().into_owned());
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            execute_engine_command,
            ingest_csv_with_progress,
            clean_table_with_progress,
            detect_address_columns,
            geocode_table_column,
            drop_table,
            analysis_chat,
            generate_analysis_sql,
            execute_analysis_sql,
            generate_visualization_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
