// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{Emitter, Manager};
use tracing::{debug, error, info};

/// Resolved at startup by `run()` → `setup` hook via Tauri's app-data dir.
/// Falls back to the legacy relative path only if setup never ran (e.g. unit tests).
static DB_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Absolute path to the current log file. Set once in `run()` setup.
static LOG_PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Active domain pack for the current session. Set once at startup.
static DOMAIN_PACK: std::sync::OnceLock<spatia_engine::DomainPack> = std::sync::OnceLock::new();

fn active_domain_pack() -> &'static spatia_engine::DomainPack {
    DOMAIN_PACK.get().unwrap_or_else(|| {
        // Fallback for tests or if setup hasn't run yet
        static DEFAULT: std::sync::OnceLock<spatia_engine::DomainPack> = std::sync::OnceLock::new();
        DEFAULT.get_or_init(spatia_engine::DomainPack::default)
    })
}

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
    table_name: String,
    stage: &'static str,
    message: String,
    percent: u8,
}

fn emit_ingest_progress(
    app: &tauri::AppHandle,
    table_name: &str,
    stage: &'static str,
    message: impl Into<String>,
    percent: u8,
) -> Result<(), String> {
    app.emit(
        "ingest-progress",
        IngestProgressEvent {
            table_name: table_name.to_string(),
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
    info!(csv_path = %csv_path, table_name = ?table_name, "ingest_csv_with_progress: starting");

    let resolved_table = table_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());

    let effective_table = resolved_table.unwrap_or("raw_staging");
    emit_ingest_progress(&app, effective_table, "started", "Starting CSV ingestion", 5)?;
    emit_ingest_progress(&app, effective_table, "reading", format!("Reading file: {csv_path}"), 30)?;

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
            info!(table = %table, "ingest_csv_with_progress: completed successfully");
            emit_ingest_progress(&app, &table, "writing", format!("Loaded table: {table}"), 85)?;
            emit_ingest_progress(&app, &table, "completed", "Ingestion complete", 100)?;
            Ok(format!("{{\"status\":\"ok\",\"table\":\"{}\"}}", table))
        }
        Err(err) => {
            error!(csv_path = %csv_path, error = %err, "ingest_csv_with_progress: failed");
            let _ = emit_ingest_progress(&app, effective_table, "failed", format!("Ingestion failed: {err}"), 100);
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
    info!(table = %table_name, "clean_table_with_progress: starting");
    let client = match spatia_ai::GeminiClient::from_env() {
        Ok(c) => c,
        Err(_) => {
            info!(table = %table_name, "clean_table_with_progress: skipped (no API key)");
            return Ok(r#"{"status":"skipped","reason":"no_api_key"}"#.to_string());
        }
    };

    // clean_table internally runs up to 3 rounds with early exit.
    // We call it exactly once — no outer loop. Previous design had an outer
    // loop of 5 × inner 3 = 15 rounds, causing near-infinite cleaning.
    emit_clean_progress(
        &app,
        "round_start",
        "Cleaning data...".to_string(),
        10,
        1,
    )?;

    let result = spatia_ai::clean_table(db_path(), &table_name, &client)
        .await
        .map_err(|e| {
            error!(table = %table_name, error = %e, "clean_table_with_progress: failed");
            e.to_string()
        })?;

    let total_statements = result.statements_applied.len();

    info!(table = %table_name, total_statements = total_statements, "clean_table_with_progress: completed");
    emit_clean_progress(
        &app,
        "completed",
        format!("{total_statements} statement(s) applied"),
        100,
        1,
    )?;

    let json = serde_json::json!({
        "status": "ok",
        "table": table_name,
        "rounds": 1,
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
    city_col: Option<String>,
    state_col: Option<String>,
    zip_col: Option<String>,
) -> Result<String, String> {
    info!(
        table = %table_name,
        col = %address_col,
        city_col = city_col.as_deref().unwrap_or(""),
        state_col = state_col.as_deref().unwrap_or(""),
        zip_col = zip_col.as_deref().unwrap_or(""),
        "geocode_table_column: starting"
    );
    spatia_engine::validate_table_name(&table_name).map_err(|e| e.to_string())?;

    // Column names must not contain double-quotes (which would break our quoting)
    if address_col.is_empty() || address_col.contains('"') {
        error!(col = %address_col, "geocode_table_column: invalid address column name");
        return Err("invalid address column name".to_string());
    }
    for opt_col in [city_col.as_deref(), state_col.as_deref(), zip_col.as_deref()]
        .iter()
        .flatten()
    {
        if opt_col.contains('"') {
            return Err(format!("invalid column name: {opt_col}"));
        }
    }

    let have_components = city_col.is_some() || state_col.is_some() || zip_col.is_some();

    emit_geocode_progress(&app, "extracting", "Extracting unique addresses...", 0)?;

    // Extract per-row address components when component columns are available,
    // or fall back to distinct address strings for the simple case.
    let components: Vec<spatia_engine::AddressComponents> = {
        let conn =
            duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;

        if have_components {
            // Build a SELECT that includes the optional component columns.
            // We use COALESCE(..., '') to avoid NULLs breaking the construction.
            let city_expr = city_col.as_deref()
                .map(|c| format!(r#"COALESCE(CAST("{c}" AS VARCHAR), '')"#))
                .unwrap_or_else(|| "''".to_string());
            let state_expr = state_col.as_deref()
                .map(|c| format!(r#"COALESCE(CAST("{c}" AS VARCHAR), '')"#))
                .unwrap_or_else(|| "''".to_string());
            let zip_expr = zip_col.as_deref()
                .map(|c| format!(r#"COALESCE(CAST("{c}" AS VARCHAR), '')"#))
                .unwrap_or_else(|| "''".to_string());

            let sql = format!(
                r#"SELECT DISTINCT "{col}", {city}, {state}, {zip}
                   FROM "{table}"
                   WHERE "{col}" IS NOT NULL"#,
                col = address_col,
                city = city_expr,
                state = state_expr,
                zip = zip_expr,
                table = table_name,
            );
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut out: Vec<spatia_engine::AddressComponents> = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let street: String = row.get(0).map_err(|e| e.to_string())?;
                let city_val: String = row.get(1).map_err(|e| e.to_string())?;
                let state_val: String = row.get(2).map_err(|e| e.to_string())?;
                let zip_val: String = row.get(3).map_err(|e| e.to_string())?;
                out.push(spatia_engine::components_from_columns(
                    &street,
                    if city_val.is_empty() { None } else { Some(city_val.as_str()) },
                    if state_val.is_empty() { None } else { Some(state_val.as_str()) },
                    if zip_val.is_empty() { None } else { Some(zip_val.as_str()) },
                ));
            }
            out
        } else {
            // Simple path: distinct non-null addresses → parse as free-text
            let sql = format!(
                r#"SELECT DISTINCT "{col}" FROM "{table}" WHERE "{col}" IS NOT NULL"#,
                col = address_col,
                table = table_name,
            );
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
            let mut out: Vec<spatia_engine::AddressComponents> = Vec::new();
            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let addr: String = row.get(0).map_err(|e| e.to_string())?;
                out.push(spatia_engine::components_from_string(&addr));
            }
            out
        }
    };

    let total_addresses = components.len();
    emit_geocode_progress(
        &app,
        "geocoding",
        format!("Geocoding {total_addresses} unique address(es)..."),
        20,
    )?;

    // geocode_batch_with_components opens and closes its own connection
    let (results, geocode_stats) =
        spatia_engine::geocode_batch_with_components(db_path(), &components)
            .map_err(|e| e.to_string())?;
    let geocoded_count = results.len();

    info!(
        table = %table_name,
        col = %address_col,
        total = geocode_stats.total,
        geocoded = geocode_stats.geocoded,
        cache_hits = geocode_stats.cache_hits,
        overture_exact = geocode_stats.overture_exact,
        local_fuzzy = geocode_stats.local_fuzzy,
        api_resolved = geocode_stats.api_resolved,
        unresolved = geocode_stats.unresolved,
        "geocode_source_breakdown"
    );

    emit_geocode_progress(
        &app,
        "geocoded",
        format!("Geocoded {geocoded_count}/{total_addresses} addresses"),
        70,
    )?;

    if !results.is_empty() {
        let conn =
            duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;

        // Add geocode columns if not already present (including _gers_id)
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
            format!(
                r#"ALTER TABLE "{}" ADD COLUMN IF NOT EXISTS _gers_id VARCHAR"#,
                table_name
            ),
        ] {
            conn.execute_batch(&alter_sql).map_err(|e| e.to_string())?;
        }

        // Build VALUES list for a temp staging table (includes gers_id)
        let values: Vec<String> = results
            .iter()
            .map(|r| {
                let gers_sql = match r.gers_id.as_deref() {
                    Some(g) => format!("'{}'", g.replace('\'', "''")),
                    None => "NULL".to_string(),
                };
                format!(
                    "('{}', {}, {}, '{}', {}, {})",
                    r.address.replace('\'', "''"),
                    r.lat,
                    r.lon,
                    r.source.replace('\'', "''"),
                    r.confidence,
                    gers_sql,
                )
            })
            .collect();

        conn.execute_batch(&format!(
            "CREATE OR REPLACE TEMP TABLE _gc AS \
             SELECT * FROM (VALUES {}) AS t(address, lat, lon, source, confidence, gers_id)",
            values.join(", "),
        ))
        .map_err(|e| e.to_string())?;

        conn.execute_batch(&format!(
            r#"UPDATE "{table}" SET _lat = g.lat, _lon = g.lon,
               _geocode_source = g.source, _geocode_confidence = g.confidence,
               _gers_id = g.gers_id
               FROM _gc g WHERE "{table}"."{col}" = g.address"#,
            table = table_name,
            col = address_col,
        ))
        .map_err(|e| e.to_string())?;

        conn.execute_batch("DROP TABLE IF EXISTS _gc")
            .map_err(|e| e.to_string())?;
    }

    info!(table = %table_name, col = %address_col, geocoded_count = geocoded_count, total = total_addresses, "geocode_table_column: completed");
    emit_geocode_progress(&app, "completed", "Geocoding complete", 100)?;

    let json = serde_json::json!({
        "status": "ok",
        "table": table_name,
        "geocoded_count": geocoded_count,
        "total_addresses": total_addresses,
        "by_source": {
            "cache": geocode_stats.cache_hits,
            "overture_exact": geocode_stats.overture_exact,
            "overture_fuzzy": geocode_stats.local_fuzzy,
            "geocodio": geocode_stats.api_resolved,
        },
        "unresolved": geocode_stats.unresolved,
    });
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

// ---- Table to GeoJSON ----

#[tauri::command]
fn table_to_geojson(table_name: String) -> Result<String, String> {
    spatia_engine::validate_table_name(&table_name).map_err(|e| e.to_string())?;

    let conn = duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;

    // Check that _lat and _lon columns exist
    let schema =
        spatia_engine::table_schema(db_path(), &table_name).map_err(|e| e.to_string())?;
    let col_names: Vec<String> = schema.iter().map(|c| c.name.clone()).collect();
    let has_lat = col_names.iter().any(|c| c == "_lat");
    let has_lon = col_names.iter().any(|c| c == "_lon");

    if !has_lat || !has_lon {
        // No geocoded columns — return empty FeatureCollection
        let fc = serde_json::json!({ "type": "FeatureCollection", "features": [] });
        return serde_json::to_string(&fc).map_err(|e| e.to_string());
    }

    // Property columns: everything except _lat and _lon themselves
    let prop_cols: Vec<String> = col_names
        .iter()
        .filter(|c| c.as_str() != "_lat" && c.as_str() != "_lon")
        .cloned()
        .collect();

    let prop_select = if prop_cols.is_empty() {
        String::new()
    } else {
        format!(
            ", {}",
            prop_cols
                .iter()
                .map(|c| format!(r#"CAST("{c}" AS VARCHAR) AS "{c}""#))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let sql = format!(
        r#"SELECT _lat, _lon{prop_select} FROM "{table}"
           WHERE _lat IS NOT NULL AND _lon IS NOT NULL
           LIMIT 10000"#,
        table = table_name,
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    let mut features: Vec<serde_json::Value> = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let lat: f64 = row.get::<_, f64>(0).map_err(|e| e.to_string())?;
        let lon: f64 = row.get::<_, f64>(1).map_err(|e| e.to_string())?;

        let mut props = serde_json::Map::new();
        for (i, col) in prop_cols.iter().enumerate() {
            let val: Option<String> = row.get(i + 2).ok();
            props.insert(
                col.clone(),
                val.map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
        }

        features.push(serde_json::json!({
            "type": "Feature",
            "geometry": {
                "type": "Point",
                "coordinates": [lon, lat]
            },
            "properties": props,
        }));
    }

    let fc = serde_json::json!({
        "type": "FeatureCollection",
        "features": features,
    });
    serde_json::to_string(&fc).map_err(|e| e.to_string())
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
    info!(table = %table_name, "analysis_chat: starting");
    if user_message.trim().is_empty() {
        return Err("user_message cannot be empty".to_string());
    }

    let schema =
        spatia_engine::table_schema(db_path(), &table_name).map_err(|err| err.to_string())?;
    let pack = active_domain_pack();
    let domain_ctx = if pack.system_prompt_extension.is_empty() {
        None
    } else {
        Some(pack.system_prompt_extension.as_str())
    };
    let system_prompt =
        spatia_ai::build_analysis_chat_system_prompt_with_domain(&table_name, &schema, domain_ctx);
    let full_prompt = format!(
        "{system}\n\n## User message\n{message}\n",
        system = system_prompt,
        message = user_message.trim()
    );

    let assistant = match spatia_ai::GeminiClient::from_env() {
        Ok(client) => client
            .generate(&full_prompt)
            .await
            .map_err(|err| {
                error!(table = %table_name, error = %err, "analysis_chat: Gemini call failed");
                err.to_string()
            })?,
        Err(_) => "Gemini is not configured. Set SPATIA_GEMINI_API_KEY to enable AI analysis chat."
            .to_string(),
    };

    info!(table = %table_name, "analysis_chat: completed");
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
    let pack = active_domain_pack();
    let domain_ctx = if pack.system_prompt_extension.is_empty() {
        None
    } else {
        Some(pack.system_prompt_extension.as_str())
    };
    let prompt =
        spatia_ai::build_analysis_sql_prompt_with_domain(&table_name, &schema, &user_goal, domain_ctx);

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
    debug!(sql = %sql, "execute_analysis_sql: executing");
    let result = spatia_engine::execute_analysis_sql_to_geojson(db_path(), &sql)
        .map_err(|err| {
            error!(sql = %sql, error = %err, "execute_analysis_sql: failed");
            err.to_string()
        })?;
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

// ---- Preview table ----

#[tauri::command]
fn preview_table(table_name: String, limit: Option<u32>) -> Result<String, String> {
    spatia_engine::validate_table_name(&table_name).map_err(|e| e.to_string())?;

    let row_limit = limit.unwrap_or(100).min(1000);

    // Get column names via engine's schema helper (uses PRAGMA safely)
    let schema =
        spatia_engine::table_schema(db_path(), &table_name).map_err(|e| e.to_string())?;
    let col_names: Vec<String> = schema.iter().map(|c| c.name.clone()).collect();

    // Query rows — cast every column to VARCHAR so non-string types (BIGINT,
    // DOUBLE, DATE, etc.) serialize correctly. The duckdb-rs driver returns Err
    // for `row.get::<_, String>(i)` on non-VARCHAR columns, which `.ok()` turns
    // into None → JSON null, making numeric columns appear empty in previews.
    let conn = duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;
    let cast_select = col_names
        .iter()
        .map(|c| format!(r#"CAST("{c}" AS VARCHAR) AS "{c}""#))
        .collect::<Vec<_>>()
        .join(", ");
    let mut stmt = conn
        .prepare(&format!(
            r#"SELECT {cast_select} FROM "{}" LIMIT {}"#,
            table_name, row_limit
        ))
        .map_err(|e| e.to_string())?;

    let mut rows_out: Vec<serde_json::Value> = Vec::new();
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;

    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let mut obj = serde_json::Map::new();
        for (i, col) in col_names.iter().enumerate() {
            let val: Option<String> = row.get(i).ok();
            match val {
                Some(v) => obj.insert(col.clone(), serde_json::Value::String(v)),
                None => obj.insert(col.clone(), serde_json::Value::Null),
            };
        }
        rows_out.push(serde_json::Value::Object(obj));
    }

    let json = serde_json::json!({
        "columns": col_names,
        "rows": rows_out,
        "total": rows_out.len(),
    });
    serde_json::to_string(&json).map_err(|e| e.to_string())
}

// ---- List tables ----

#[tauri::command]
fn list_tables() -> Result<String, String> {
    let conn = duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = 'main' AND table_type = 'BASE TABLE' \
             AND table_name NOT IN ('geocode_cache', 'analysis_result') \
             ORDER BY table_name",
        )
        .map_err(|e| e.to_string())?;

    let mut rows = stmt
        .query([])
        .map_err(|e| e.to_string())?;
    let mut tables = Vec::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let name: String = row.get::<_, String>(0).map_err(|e| e.to_string())?;
        tables.push(serde_json::json!({ "name": name }));
    }

    serde_json::to_string(&serde_json::json!({ "tables": tables })).map_err(|e| e.to_string())
}

// ---- Ingest file pipeline ----

#[tauri::command]
async fn ingest_file_pipeline(
    app: tauri::AppHandle,
    csv_path: String,
    table_name: String,
) -> Result<String, String> {
    // Run the entire pipeline on a blocking thread to avoid deadlocking
    // the async runtime with DuckDB's synchronous file-level locks.
    // The only async part (Gemini API calls) uses Handle::block_on inside.
    let handle = tokio::runtime::Handle::current();

    let join_result = tokio::task::spawn_blocking(move || {
        // Step 1: ingest
        emit_ingest_progress(&app, &table_name, "started", "Starting CSV ingestion", 5)?;
        emit_ingest_progress(&app, &table_name, "reading", format!("Reading file: {csv_path}"), 30)?;

        spatia_engine::ingest_csv_to_table(db_path(), &csv_path, &table_name)
            .map_err(|e| e.to_string())?;

        emit_ingest_progress(&app, &table_name, "writing", format!("Loaded table: {table_name}"), 50)?;

        // Step 2: AI clean
        emit_ingest_progress(&app, &table_name, "cleaning", "Starting AI clean...", 55)?;

        // clean_table internally runs up to 3 rounds with early exit.
        // Call it once — no outer loop needed.
        let clean_summary = match spatia_ai::GeminiClient::from_env() {
            Ok(client) => {
                let result = handle.block_on(
                    spatia_ai::clean_table(db_path(), &table_name, &client),
                )
                .map_err(|e| e.to_string())?;

                let total_statements = result.statements_applied.len();
                format!("{total_statements} statement(s) applied")
            }
            Err(_) => "skipped (no API key)".to_string(),
        };

        emit_ingest_progress(&app, &table_name, "detecting", "Detecting address columns...", 85)?;

        // Step 3: detect address columns
        let schema =
            spatia_engine::table_schema(db_path(), &table_name).map_err(|e| e.to_string())?;
        let address_columns: Vec<String> = schema
            .into_iter()
            .filter(|col| {
                let name = col.name.to_lowercase();
                let dtype = col.data_type.to_lowercase();
                if !dtype.contains("varchar")
                    && !dtype.contains("text")
                    && !dtype.contains("string")
                {
                    return false;
                }
                if ["ip", "email", "url", "web"]
                    .iter()
                    .any(|p| name.contains(p))
                {
                    return false;
                }
                ["address", "addr", "street", "location", "place"]
                    .iter()
                    .any(|p| name.contains(p))
                    || ["city", "zip", "postal", "suburb", "neighbourhood"]
                        .contains(&name.as_str())
            })
            .map(|col| col.name)
            .collect();

        // Get row count
        let conn = duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;
        let row_count: i64 = conn
            .query_row(
                &format!(r#"SELECT COUNT(*) FROM "{}""#, table_name),
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;

        // If address columns were detected, stop here and let the user confirm
        // geocoding via the UI confirmation card. If no address columns, the
        // pipeline is complete.
        let pipeline_status = if address_columns.is_empty() { "done" } else { "ready" };

        emit_ingest_progress(&app, &table_name, "completed", "Pipeline complete", 100)?;

        let json = serde_json::json!({
            "status": pipeline_status,
            "table": table_name,
            "row_count": row_count,
            "clean_summary": clean_summary,
            "address_columns": address_columns,
        });
        serde_json::to_string(&json).map_err(|e| e.to_string())
    })
    .await;

    match join_result {
        Ok(inner) => inner,
        Err(e) => Err(e.to_string()),
    }
}


// ---- Chat turn ----

/// Validates the AI-chosen visualization type against the actual execution result.
///
/// Returns the original type if it is appropriate for the data, or overrides to
/// "table" when the choice is clearly wrong (e.g. the AI asked for a scatter plot
/// but the result has no coordinate columns, or asked for a chart but got 0 rows).
///
/// `row_count` is the number of GeoJSON features (rows) in the result.
/// `columns` and `first_row` come from the tabular preview.
///
/// Overrides are conservative — only applied when the type is unambiguously wrong.
fn validate_visualization_type(
    requested: &str,
    row_count: usize,
    columns: &[String],
    first_row: Option<&Vec<Value>>,
) -> String {
    let map_types = ["scatter", "heatmap", "hexbin"];
    let chart_types = ["bar_chart", "pie_chart", "histogram"];

    // If the result has no rows, non-table types can't render anything meaningful.
    if row_count == 0 && (map_types.contains(&requested) || chart_types.contains(&requested)) {
        info!(
            "visualization_type override: {} -> table (reason: 0 rows in result)",
            requested
        );
        return "table".to_string();
    }

    // Map-based types require lat/lon columns. If none are present, fall back to table.
    if map_types.contains(&requested) {
        let coord_names = ["lat", "latitude", "_lat", "lon", "lng", "longitude", "_lon"];
        let has_coords = columns
            .iter()
            .any(|c| coord_names.iter().any(|name| c.eq_ignore_ascii_case(name)));
        if !has_coords {
            info!(
                "visualization_type override: {} -> table (reason: no lat/lon columns in result)",
                requested
            );
            return "table".to_string();
        }
    }

    // bar_chart, pie_chart, and histogram require at least one numeric column.
    // The tabular rows are all JSON strings after the CAST-to-VARCHAR pass, so
    // we try str::parse::<f64>() on each cell in the first row as a heuristic.
    if ["bar_chart", "pie_chart", "histogram"].contains(&requested) {
        let has_numeric = match first_row {
            None => false,
            Some(row) => row.iter().any(|cell| {
                cell.as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .is_some()
            }),
        };
        if !has_numeric {
            info!(
                "visualization_type override: {} -> table (reason: no numeric column in result)",
                requested
            );
            return "table".to_string();
        }
    }

    requested.to_string()
}

/// Serialisable tabular preview included in chat turn responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TabularResultPayload {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatTurnResult {
    message: String,
    sql: Option<String>,
    geojson: Option<Value>,
    map_actions: Vec<Value>,
    row_count: Option<usize>,
    total_count: Option<usize>,
    result_rows: Option<TabularResultPayload>,
    visualization_type: String,
    /// True when the first SQL attempt failed and a second AI call produced the
    /// SQL that ultimately succeeded (or also failed).
    retry_attempted: bool,
}

#[tauri::command]
async fn chat_turn(
    table_names: Vec<String>,
    user_message: String,
    conversation_history: Vec<serde_json::Value>,
) -> Result<String, String> {
    info!(tables = ?table_names, history_len = conversation_history.len(), "chat_turn: starting");
    if user_message.trim().is_empty() {
        return Err("user_message cannot be empty".to_string());
    }

    // Fetch schemas for all tables
    let mut table_schemas = Vec::new();
    for name in &table_names {
        match spatia_engine::table_schema(db_path(), name) {
            Ok(schema) => table_schemas.push((name.clone(), schema)),
            Err(e) => {
                error!(table = %name, error = %e, "chat_turn: failed to get schema");
                return Err(format!("Failed to get schema for {name}: {e}"));
            }
        }
    }

    // Build domain context (prompt extension + detected columns)
    let pack = active_domain_pack();
    let domain_context = {
        let mut ctx = pack.system_prompt_extension.clone();
        if !pack.column_detection_rules.is_empty() {
            let all_columns: Vec<spatia_engine::TableColumn> = table_schemas
                .iter()
                .flat_map(|(_, schema)| schema.iter().cloned())
                .collect();
            let detected =
                spatia_engine::detect_domain_columns(&all_columns, &pack.column_detection_rules);
            let annotations = spatia_engine::format_domain_column_annotations(&detected);
            if !annotations.is_empty() {
                ctx.push_str(&annotations);
            }
        }
        if ctx.is_empty() { None } else { Some(ctx) }
    };

    // Build unified prompt
    let prompt = spatia_ai::build_unified_chat_prompt_with_domain(
        &table_schemas,
        &user_message,
        &conversation_history,
        None,
        domain_context.as_deref(),
    );

    // Call Gemini with JSON mode
    let client = match spatia_ai::GeminiClient::from_env() {
        Ok(c) => c,
        Err(_) => {
            let result = ChatTurnResult {
                message: "Gemini is not configured. Set SPATIA_GEMINI_API_KEY to enable AI analysis.".to_string(),
                sql: None,
                geojson: None,
                map_actions: vec![],
                row_count: None,
                total_count: None,
                result_rows: None,
                visualization_type: "scatter".to_string(),
                retry_attempted: false,
            };
            return serde_json::to_string(&result).map_err(|e| e.to_string());
        }
    };

    let response_text = client
        .generate_json(&prompt)
        .await
        .map_err(|e| {
            error!(error = %e, "chat_turn: Gemini JSON call failed");
            e.to_string()
        })?;

    // Parse JSON response
    let parsed: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
        error!(error = %e, raw_response = %response_text, "chat_turn: failed to parse AI response as JSON");
        format!("Failed to parse AI response as JSON: {e}\nRaw: {response_text}")
    })?;

    let message = parsed
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("No response")
        .to_string();

    let sql = parsed
        .get("sql")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let map_actions: Vec<Value> = parsed
        .get("map_actions")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let visualization_type = parsed
        .get("visualization_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "scatter".to_string());

    // Execute SQL if present, with one automatic retry on DuckDB execution errors.
    // Retry is skipped for validation errors (blocked keywords / bad prefix) and
    // when no SQL was generated.
    let (geojson, row_count, total_count, result_rows, retry_attempted, final_sql) =
        if let Some(ref sql_str) = sql {
            debug!(sql = %sql_str, "chat_turn: executing analysis SQL");
            match spatia_engine::execute_analysis_sql_to_geojson(db_path(), sql_str) {
                Ok(engine_result) => {
                    info!(
                        row_count = engine_result.row_count,
                        "chat_turn: SQL executed successfully"
                    );
                    let tabular = TabularResultPayload {
                        columns: engine_result.tabular.columns,
                        rows: engine_result.tabular.rows,
                        truncated: engine_result.tabular.truncated,
                    };
                    (
                        Some(engine_result.geojson),
                        Some(engine_result.row_count),
                        Some(engine_result.total_count),
                        Some(tabular),
                        false,
                        sql.clone(),
                    )
                }
                Err(first_err) => {
                    let first_err_str = first_err.to_string();
                    error!(
                        sql = %sql_str,
                        error = %first_err_str,
                        "chat_turn: SQL execution failed, attempting retry"
                    );

                    // Only retry on DuckDB execution errors, not validation errors.
                    // Validation errors ("analysis SQL must start with" / "disallowed
                    // statement") indicate a structural mismatch — retrying would not
                    // help without changing the output format.
                    let is_validation_error =
                        first_err_str.contains("analysis SQL must start with")
                            || first_err_str.contains("disallowed statement");

                    if is_validation_error {
                        error!(
                            "chat_turn: skipping retry — original error was a validation error"
                        );
                        let result = ChatTurnResult {
                            message: format!(
                                "{message}\n\nThe generated SQL was blocked by a safety check and could not be executed."
                            ),
                            sql: sql.clone(),
                            geojson: None,
                            map_actions,
                            row_count: None,
                            total_count: None,
                            result_rows: None,
                            visualization_type,
                            retry_attempted: false,
                        };
                        return serde_json::to_string(&result).map_err(|e| e.to_string());
                    }

                    // Build a retry prompt and ask Gemini for a corrected SQL statement.
                    let retry_prompt = spatia_ai::build_analysis_retry_prompt_with_domain(
                        &user_message,
                        &table_schemas,
                        sql_str,
                        &first_err_str,
                        None,
                        domain_context.as_deref(),
                    );

                    let retry_sql_raw = match client.generate(&retry_prompt).await {
                        Ok(text) => text,
                        Err(retry_gemini_err) => {
                            error!(
                                error = %retry_gemini_err,
                                "chat_turn: retry Gemini call failed"
                            );
                            let result = ChatTurnResult {
                                message: format!(
                                    "{message}\n\nThe SQL could not be executed and the automatic correction also failed. Please rephrase your question."
                                ),
                                sql: sql.clone(),
                                geojson: None,
                                map_actions,
                                row_count: None,
                                total_count: None,
                                result_rows: None,
                                visualization_type,
                                retry_attempted: true,
                            };
                            return serde_json::to_string(&result).map_err(|e| e.to_string());
                        }
                    };

                    // Strip markdown fences if the model wrapped its response.
                    let retry_sql = retry_sql_raw
                        .trim()
                        .trim_start_matches("```sql")
                        .trim_start_matches("```")
                        .trim_end_matches("```")
                        .trim()
                        .to_string();

                    info!(retry_sql = %retry_sql, "chat_turn: retrying with corrected SQL");

                    match spatia_engine::execute_analysis_sql_to_geojson(db_path(), &retry_sql) {
                        Ok(engine_result) => {
                            info!(
                                row_count = engine_result.row_count,
                                "chat_turn: retry SQL executed successfully"
                            );
                            let tabular = TabularResultPayload {
                                columns: engine_result.tabular.columns,
                                rows: engine_result.tabular.rows,
                                truncated: engine_result.tabular.truncated,
                            };
                            (
                                Some(engine_result.geojson),
                                Some(engine_result.row_count),
                                Some(engine_result.total_count),
                                Some(tabular),
                                true,
                                Some(retry_sql),
                            )
                        }
                        Err(retry_err) => {
                            error!(
                                first_error = %first_err_str,
                                retry_error = %retry_err,
                                "chat_turn: retry SQL also failed"
                            );
                            let result = ChatTurnResult {
                                message: format!(
                                    "{message}\n\nThe SQL could not be executed even after an automatic correction attempt. Please rephrase your question."
                                ),
                                sql: Some(retry_sql),
                                geojson: None,
                                map_actions,
                                row_count: None,
                                total_count: None,
                                result_rows: None,
                                visualization_type,
                                retry_attempted: true,
                            };
                            return serde_json::to_string(&result).map_err(|e| e.to_string());
                        }
                    }
                }
            }
        } else {
            (None, None, None, None, false, sql.clone())
        };

    // Validate the AI's visualization choice against the actual data. Only run
    // the check when SQL was executed and we have a result to inspect. When there
    // is no SQL (conversational turn), leave the type unchanged.
    let validated_visualization_type = if let (Some(rc), Some(ref rows_payload)) =
        (row_count, &result_rows)
    {
        let first_row = rows_payload.rows.first();
        validate_visualization_type(&visualization_type, rc, &rows_payload.columns, first_row)
    } else {
        visualization_type
    };

    let result = ChatTurnResult {
        message,
        sql: final_sql,
        geojson,
        map_actions,
        row_count,
        total_count,
        result_rows,
        visualization_type: validated_visualization_type,
        retry_attempted,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ---- Log path ----

#[tauri::command]
fn get_log_path() -> Result<String, String> {
    Ok(LOG_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| "logs/spatia.log".to_string()))
}

// ---- Export commands ----

#[tauri::command]
fn export_table_csv(table_name: String, file_path: String) -> Result<(), String> {
    let conn = duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;
    spatia_engine::export_table_csv(&conn, &table_name, &file_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn export_analysis_geojson(file_path: String) -> Result<(), String> {
    let conn = duckdb::Connection::open(db_path()).map_err(|e| e.to_string())?;
    spatia_engine::export_analysis_geojson(&conn, &file_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_file(file_path: String, data: String) -> Result<(), String> {
    // Strip data URL prefix if present
    let b64 = data
        .strip_prefix("data:image/png;base64,")
        .unwrap_or(&data);
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| e.to_string())?;
    std::fs::write(&file_path, bytes).map_err(|e| e.to_string())
}

// ---- Settings / API key management ----

#[tauri::command]
fn save_api_key(app: tauri::AppHandle, key_name: String, key_value: String) -> Result<(), String> {
    use tauri_plugin_store::StoreExt;
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set(&key_name, serde_json::json!(key_value));
    // Also update the process env var so the current session picks up the key
    let env_name = match key_name.as_str() {
        "gemini_api_key" => Some("SPATIA_GEMINI_API_KEY"),
        "geocodio_api_key" => Some("SPATIA_GEOCODIO_API_KEY"),
        _ => None,
    };
    if let Some(name) = env_name {
        std::env::set_var(name, &key_value);
    }
    Ok(())
}

#[tauri::command]
fn get_api_key(app: tauri::AppHandle, key_name: String) -> Result<Option<String>, String> {
    use tauri_plugin_store::StoreExt;
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let val = store.get(&key_name).and_then(|v| v.as_str().map(|s| s.to_string()));
    Ok(val)
}

#[tauri::command]
fn delete_api_key(app: tauri::AppHandle, key_name: String) -> Result<(), String> {
    use tauri_plugin_store::StoreExt;
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let _ = store.delete(&key_name);
    let env_name = match key_name.as_str() {
        "gemini_api_key" => Some("SPATIA_GEMINI_API_KEY"),
        "geocodio_api_key" => Some("SPATIA_GEOCODIO_API_KEY"),
        _ => None,
    };
    if let Some(name) = env_name {
        std::env::remove_var(name);
    }
    Ok(())
}

// ---- API config check ----

#[derive(Debug, Clone, Serialize)]
struct ApiConfigResponse {
    gemini: bool,
    geocodio: bool,
}

#[tauri::command]
fn check_api_config() -> Result<String, String> {
    let gemini = std::env::var("SPATIA_GEMINI_API_KEY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let geocodio = std::env::var("SPATIA_GEOCODIO_API_KEY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    serde_json::to_string(&ApiConfigResponse { gemini, geocodio }).map_err(|e| e.to_string())
}

// ---- Domain pack config ----

#[tauri::command]
fn get_domain_pack_config() -> Result<String, String> {
    let pack = active_domain_pack();
    serde_json::to_string(pack).map_err(|e| e.to_string())
}

// ---- Debug snapshot ----

/// Writes a JSON snapshot of the frontend Zustand store to
/// `<project-root>/scripts/screenshots/ui-state.json`.
///
/// Called exclusively from the frontend's `window.__spatia_debug_snapshot()`.
/// The command is compiled only in debug builds (`#[cfg(debug_assertions)]`).
#[cfg(debug_assertions)]
#[tauri::command]
fn write_debug_snapshot(data: String) -> Result<(), String> {
    // Resolve the output path relative to the Cargo workspace root.
    // At runtime (both `pnpm tauri dev` and `cargo test`) the process cwd is
    // typically `src-tauri/`, so we go one level up to reach the repo root.
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;

    // Try `<cwd>/scripts/screenshots` first; if `scripts` doesn't exist there,
    // try the parent directory (covers both `src-tauri/` and repo-root cwds).
    let screenshots_dir = {
        let candidate = cwd.join("scripts").join("screenshots");
        if candidate.parent().map(|p| p.exists()).unwrap_or(false) {
            candidate
        } else {
            cwd.join("..").join("scripts").join("screenshots")
        }
    };

    std::fs::create_dir_all(&screenshots_dir).map_err(|e| e.to_string())?;

    let out_path = screenshots_dir.join("ui-state.json");
    std::fs::write(&out_path, data).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load .env from the project root (where `pnpm tauri dev` runs)
    let _ = dotenvy::dotenv();

    // ---- Tracing / logging setup ----
    // Determine a stable logs directory: prefer next to the DuckDB data dir
    // (resolved later in setup), but initialise with cwd-relative fallback so
    // logs are available from the very first line of code.
    let logs_dir = {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        cwd.join("logs")
    };
    let _ = std::fs::create_dir_all(&logs_dir);

    let log_file_path = logs_dir.join("spatia.log");
    let _ = LOG_PATH.set(log_file_path.to_string_lossy().into_owned());

    // Rolling file appender — new file each day, kept in `logs/`
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "spatia.log");
    let (non_blocking_file, _file_guard) = tracing_appender::non_blocking(file_appender);

    // EnvFilter: default INFO for file; honour RUST_LOG env var for overrides
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    use tracing_subscriber::prelude::*;
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_file)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true);

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(stderr_layer)
        .init();

    // Leak the guard so the non-blocking writer flushes on process exit.
    std::mem::forget(_file_guard);

    info!("spatia starting up; log file: {}", log_file_path.display());

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
                info!(db_path = %db.display(), "spatia: resolved DuckDB path");
            }

            // Inject stored API keys into env vars (if not already set)
            {
                use tauri_plugin_store::StoreExt;
                if let Ok(store) = app.store("settings.json") {
                    let keys = [
                        ("gemini_api_key", "SPATIA_GEMINI_API_KEY"),
                        ("geocodio_api_key", "SPATIA_GEOCODIO_API_KEY"),
                    ];
                    for (store_key, env_key) in keys {
                        let env_present = std::env::var(env_key)
                            .map(|v| !v.trim().is_empty())
                            .unwrap_or(false);
                        if !env_present {
                            if let Some(val) = store.get(store_key).and_then(|v| v.as_str().map(|s| s.to_string())) {
                                if !val.trim().is_empty() {
                                    std::env::set_var(env_key, &val);
                                    info!(key = %env_key, "spatia: loaded API key from store");
                                }
                            }
                        }
                    }
                }
            }

            // Resolve the active domain pack from SPATIA_DOMAIN_PACK env var.
            let pack = spatia_engine::DomainPack::from_env();
            info!(domain_pack = %pack.id, "spatia: active domain pack");
            let _ = DOMAIN_PACK.set(pack);

            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler({
            // `write_debug_snapshot` is only compiled in debug builds.
            // We select the handler list at compile time to keep the release
            // binary free of any debug-only surface area.
            #[cfg(debug_assertions)]
            {
                tauri::generate_handler![
                    greet,
                    execute_engine_command,
                    ingest_csv_with_progress,
                    clean_table_with_progress,
                    detect_address_columns,
                    geocode_table_column,
                    drop_table,
                    table_to_geojson,
                    analysis_chat,
                    generate_analysis_sql,
                    execute_analysis_sql,
                    generate_visualization_command,
                    list_tables,
                    preview_table,
                    ingest_file_pipeline,
                    chat_turn,
                    check_api_config,
                    get_log_path,
                    get_domain_pack_config,
                    export_table_csv,
                    export_analysis_geojson,
                    save_file,
                    save_api_key,
                    get_api_key,
                    delete_api_key,
                    write_debug_snapshot
                ]
            }
            #[cfg(not(debug_assertions))]
            {
                tauri::generate_handler![
                    greet,
                    execute_engine_command,
                    ingest_csv_with_progress,
                    clean_table_with_progress,
                    detect_address_columns,
                    geocode_table_column,
                    drop_table,
                    table_to_geojson,
                    analysis_chat,
                    generate_analysis_sql,
                    execute_analysis_sql,
                    generate_visualization_command,
                    list_tables,
                    preview_table,
                    ingest_file_pipeline,
                    chat_turn,
                    check_api_config,
                    get_log_path,
                    get_domain_pack_config,
                    export_table_csv,
                    export_analysis_geojson,
                    save_file,
                    save_api_key,
                    get_api_key,
                    delete_api_key
                ]
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
