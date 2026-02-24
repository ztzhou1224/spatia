// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

use serde::{Deserialize, Serialize};
use tauri::Emitter;

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
    db_path: String,
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
        spatia_engine::ingest_csv_to_table(&db_path, &csv_path, table)
            .map(|_| table.to_string())
            .map_err(|err| err.to_string())
    } else {
        spatia_engine::ingest_csv(&db_path, &csv_path)
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

#[tauri::command]
async fn analysis_chat(
    db_path: String,
    table_name: String,
    user_message: String,
) -> Result<String, String> {
    if user_message.trim().is_empty() {
        return Err("user_message cannot be empty".to_string());
    }

    let schema = spatia_engine::table_schema(&db_path, &table_name).map_err(|err| err.to_string())?;
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
        Err(_) => {
            "Gemini is not configured. Set SPATIA_GEMINI_API_KEY to enable AI analysis chat.".to_string()
        }
    };

    let payload = AnalysisChatResponse {
        assistant,
        system_prompt,
    };
    serde_json::to_string(&payload).map_err(|err| err.to_string())
}

#[tauri::command]
async fn generate_analysis_sql(
    db_path: String,
    table_name: String,
    user_goal: String,
) -> Result<String, String> {
    if user_goal.trim().is_empty() {
        return Err("user_goal cannot be empty".to_string());
    }

    let schema = spatia_engine::table_schema(&db_path, &table_name).map_err(|err| err.to_string())?;
    let prompt = spatia_ai::build_analysis_sql_prompt(&table_name, &schema, &user_goal);

    let sql = match spatia_ai::GeminiClient::from_env() {
        Ok(client) => client.generate(&prompt).await.map_err(|err| err.to_string())?,
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
fn execute_analysis_sql(db_path: String, sql: String) -> Result<String, String> {
    let result = spatia_engine::execute_analysis_sql_to_geojson(&db_path, &sql)
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
            let text = client.generate(&prompt).await.map_err(|err| err.to_string())?;
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            execute_engine_command,
            ingest_csv_with_progress,
            analysis_chat,
            generate_analysis_sql,
            execute_analysis_sql,
            generate_visualization_command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
