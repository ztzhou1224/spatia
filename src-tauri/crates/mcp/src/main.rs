//! Spatia MCP Server
//!
//! An MCP (Model Context Protocol) server that exposes all Spatia engine
//! commands as tools over JSON-RPC 2.0 on stdio.  Any MCP-compatible
//! AI client (Claude Desktop, Cursor, etc.) can connect by launching this
//! binary and communicating over stdin / stdout.
//!
//! ## Supported tools
//! | Tool              | Engine command              |
//! |-------------------|-----------------------------|
//! | `ingest_csv`      | `ingest <db> <csv> [table]` |
//! | `get_schema`      | `schema <db> <table>`       |
//! | `overture_extract`| `overture_extract …`        |
//! | `overture_search` | `overture_search …`         |
//! | `overture_geocode`| `overture_geocode …`        |
//!
//! ## Transport
//! The server reads newline-delimited JSON from stdin and writes
//! newline-delimited JSON to stdout.  Each line is one JSON-RPC 2.0 message.
//! Notifications (messages without an `id`) are silently ignored.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use spatia_engine::execute_command;

// ── JSON-RPC 2.0 types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Request {
    #[allow(dead_code)]
    jsonrpc: String,
    /// Absent for notifications; present for requests.
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct Response {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl Response {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(_) => break,
        };

        if let Some(resp) = handle_line(&line) {
            let serialized = serde_json::to_string(&resp).unwrap_or_else(|_| {
                r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal serialization error"}}"#.to_string()
            });
            let _ = writeln!(out, "{serialized}");
            let _ = out.flush();
        }
    }
}

// ── Message dispatch ─────────────────────────────────────────────────────────

fn handle_line(line: &str) -> Option<Response> {
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return Some(Response::err(
                Value::Null,
                -32700,
                format!("Parse error: {e}"),
            ));
        }
    };

    // Notifications have no `id` and require no response.
    let id = req.id.clone()?;

    Some(match req.method.as_str() {
        "initialize" => match handle_initialize(&req.params) {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::err(id, -32603, e),
        },
        "ping" => Response::ok(id, json!({})),
        "tools/list" => match handle_tools_list() {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::err(id, -32603, e),
        },
        "tools/call" => match handle_tools_call(&req.params) {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::err(id, -32602, e),
        },
        unknown => Response::err(id, -32601, format!("Method not found: {unknown}")),
    })
}

// ── MCP method handlers ───────────────────────────────────────────────────────

fn handle_initialize(_params: &Value) -> Result<Value, String> {
    Ok(json!({
        "protocolVersion": "2024-11-05",
        "serverInfo": {
            "name": "spatia-mcp",
            "version": env!("CARGO_PKG_VERSION")
        },
        "capabilities": {
            "tools": {}
        }
    }))
}

fn handle_tools_list() -> Result<Value, String> {
    Ok(json!({
        "tools": [
            {
                "name": "ingest_csv",
                "description": "Load a CSV file into a DuckDB database table. Returns JSON with status and table name.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "db_path": {
                            "type": "string",
                            "description": "Path to the DuckDB database file"
                        },
                        "csv_path": {
                            "type": "string",
                            "description": "Path to the CSV file to ingest"
                        },
                        "table_name": {
                            "type": "string",
                            "description": "Optional target table name. Defaults to raw_staging when omitted."
                        }
                    },
                    "required": ["db_path", "csv_path"]
                }
            },
            {
                "name": "get_schema",
                "description": "Get the column schema of a table in a DuckDB database. Returns a JSON array of column definitions.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "db_path": {
                            "type": "string",
                            "description": "Path to the DuckDB database file"
                        },
                        "table_name": {
                            "type": "string",
                            "description": "Name of the table to inspect"
                        }
                    },
                    "required": ["db_path", "table_name"]
                }
            },
            {
                "name": "overture_extract",
                "description": "Extract Overture GIS data within a bounding box from the Overture parquet release into a local DuckDB table.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "db_path": {
                            "type": "string",
                            "description": "Path to the DuckDB database file"
                        },
                        "theme": {
                            "type": "string",
                            "description": "Overture theme (e.g. places, addresses, buildings, base, transportation)"
                        },
                        "item_type": {
                            "type": "string",
                            "description": "Overture item type within the theme (e.g. place, address, building)"
                        },
                        "bbox": {
                            "type": "string",
                            "description": "Bounding box as xmin,ymin,xmax,ymax (e.g. -122.4,47.5,-122.2,47.7)"
                        },
                        "table_name": {
                            "type": "string",
                            "description": "Optional name for the output DuckDB table"
                        }
                    },
                    "required": ["db_path", "theme", "item_type", "bbox"]
                }
            },
            {
                "name": "overture_search",
                "description": "Search Overture data in a local DuckDB table by name or keyword. Returns ranked results as a JSON array.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "db_path": {
                            "type": "string",
                            "description": "Path to the DuckDB database file"
                        },
                        "table_name": {
                            "type": "string",
                            "description": "Name of the Overture table to search"
                        },
                        "query": {
                            "type": "string",
                            "description": "Search query string"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results to return. Defaults to 20.",
                            "minimum": 1
                        }
                    },
                    "required": ["db_path", "table_name", "query"]
                }
            },
            {
                "name": "overture_geocode",
                "description": "Geocode an address using local Overture addresses data in DuckDB. Returns ranked coordinate results as a JSON array.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "db_path": {
                            "type": "string",
                            "description": "Path to the DuckDB database file"
                        },
                        "table_name": {
                            "type": "string",
                            "description": "Name of the Overture addresses table"
                        },
                        "query": {
                            "type": "string",
                            "description": "Address query to geocode"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results to return. Defaults to 20.",
                            "minimum": 1
                        }
                    },
                    "required": ["db_path", "table_name", "query"]
                }
            }
        ]
    }))
}

fn handle_tools_call(params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing required parameter: name".to_string())?;

    let args = params.get("arguments").unwrap_or(&Value::Null);

    let command = build_command(name, args)?;

    Ok(match execute_command(&command) {
        Ok(text) => json!({
            "content": [{ "type": "text", "text": text }]
        }),
        Err(e) => json!({
            "content": [{ "type": "text", "text": e.to_string() }],
            "isError": true
        }),
    })
}

// ── Command builder ───────────────────────────────────────────────────────────

/// Converts MCP tool arguments into the engine's string-command format.
///
/// Arguments containing whitespace are quoted. Double-quoted strings are
/// wrapped with single quotes and vice-versa. An error is returned if a
/// string contains both quote types (the engine tokenizer has no escape
/// support).
fn build_command(tool_name: &str, args: &Value) -> Result<String, String> {
    match tool_name {
        "ingest_csv" => {
            let db = require_str(args, "db_path")?;
            let csv = require_str(args, "csv_path")?;
            if let Some(table) = args.get("table_name").and_then(Value::as_str) {
                Ok(format!(
                    "ingest {} {} {}",
                    quote(db)?,
                    quote(csv)?,
                    quote(table)?
                ))
            } else {
                Ok(format!("ingest {} {}", quote(db)?, quote(csv)?))
            }
        }
        "get_schema" => {
            let db = require_str(args, "db_path")?;
            let table = require_str(args, "table_name")?;
            Ok(format!("schema {} {}", quote(db)?, quote(table)?))
        }
        "overture_extract" => {
            let db = require_str(args, "db_path")?;
            let theme = require_str(args, "theme")?;
            let item_type = require_str(args, "item_type")?;
            let bbox = require_str(args, "bbox")?;
            if let Some(table) = args.get("table_name").and_then(Value::as_str) {
                Ok(format!(
                    "overture_extract {} {} {} {} {}",
                    quote(db)?,
                    quote(theme)?,
                    quote(item_type)?,
                    quote(bbox)?,
                    quote(table)?
                ))
            } else {
                Ok(format!(
                    "overture_extract {} {} {} {}",
                    quote(db)?,
                    quote(theme)?,
                    quote(item_type)?,
                    quote(bbox)?
                ))
            }
        }
        "overture_search" => {
            let db = require_str(args, "db_path")?;
            let table = require_str(args, "table_name")?;
            let query = require_str(args, "query")?;
            if let Some(limit) = args.get("limit").and_then(Value::as_u64) {
                Ok(format!(
                    "overture_search {} {} {} {}",
                    quote(db)?,
                    quote(table)?,
                    quote(query)?,
                    limit
                ))
            } else {
                Ok(format!(
                    "overture_search {} {} {}",
                    quote(db)?,
                    quote(table)?,
                    quote(query)?
                ))
            }
        }
        "overture_geocode" => {
            let db = require_str(args, "db_path")?;
            let table = require_str(args, "table_name")?;
            let query = require_str(args, "query")?;
            if let Some(limit) = args.get("limit").and_then(Value::as_u64) {
                Ok(format!(
                    "overture_geocode {} {} {} {}",
                    quote(db)?,
                    quote(table)?,
                    quote(query)?,
                    limit
                ))
            } else {
                Ok(format!(
                    "overture_geocode {} {} {}",
                    quote(db)?,
                    quote(table)?,
                    quote(query)?
                ))
            }
        }
        unknown => Err(format!("Unknown tool: {unknown}")),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing required argument: {key}"))
}

/// Wraps a string in quotes if it contains whitespace so the engine
/// tokenizer treats it as a single token.
///
/// Strings with whitespace are wrapped in double quotes unless the string
/// already contains a double quote, in which case single quotes are used.
/// Returns an error if the string contains *both* single and double quotes,
/// since the engine tokenizer has no escape-sequence support.
fn quote(s: &str) -> Result<String, String> {
    if !s.chars().any(char::is_whitespace) {
        return Ok(s.to_string());
    }
    let has_double = s.contains('"');
    let has_single = s.contains('\'');
    match (has_double, has_single) {
        (false, _) => Ok(format!("\"{s}\"")),
        (true, false) => Ok(format!("'{s}'")),
        (true, true) => Err(format!(
            "Argument contains both single and double quotes and cannot be safely quoted: {s:?}"
        )),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── build_command ────────────────────────────────────────────────────────

    #[test]
    fn build_ingest_without_table() {
        let args = json!({ "db_path": "/tmp/test.duckdb", "csv_path": "/tmp/data.csv" });
        let cmd = build_command("ingest_csv", &args).unwrap();
        assert_eq!(cmd, "ingest /tmp/test.duckdb /tmp/data.csv");
    }

    #[test]
    fn build_ingest_with_table() {
        let args = json!({
            "db_path": "/tmp/test.duckdb",
            "csv_path": "/tmp/data.csv",
            "table_name": "places"
        });
        let cmd = build_command("ingest_csv", &args).unwrap();
        assert_eq!(cmd, "ingest /tmp/test.duckdb /tmp/data.csv places");
    }

    #[test]
    fn build_get_schema() {
        let args = json!({ "db_path": "/tmp/test.duckdb", "table_name": "raw_staging" });
        let cmd = build_command("get_schema", &args).unwrap();
        assert_eq!(cmd, "schema /tmp/test.duckdb raw_staging");
    }

    #[test]
    fn build_overture_extract_without_table() {
        let args = json!({
            "db_path": "/tmp/spatia.duckdb",
            "theme": "places",
            "item_type": "place",
            "bbox": "-122.4,47.5,-122.2,47.7"
        });
        let cmd = build_command("overture_extract", &args).unwrap();
        assert_eq!(
            cmd,
            "overture_extract /tmp/spatia.duckdb places place -122.4,47.5,-122.2,47.7"
        );
    }

    #[test]
    fn build_overture_extract_with_table() {
        let args = json!({
            "db_path": "/tmp/spatia.duckdb",
            "theme": "places",
            "item_type": "place",
            "bbox": "-122.4,47.5,-122.2,47.7",
            "table_name": "places_wa"
        });
        let cmd = build_command("overture_extract", &args).unwrap();
        assert_eq!(
            cmd,
            "overture_extract /tmp/spatia.duckdb places place -122.4,47.5,-122.2,47.7 places_wa"
        );
    }

    #[test]
    fn build_overture_search_without_limit() {
        let args = json!({
            "db_path": "/tmp/spatia.duckdb",
            "table_name": "places_wa",
            "query": "lincoln park"
        });
        let cmd = build_command("overture_search", &args).unwrap();
        assert_eq!(
            cmd,
            "overture_search /tmp/spatia.duckdb places_wa \"lincoln park\""
        );
    }

    #[test]
    fn build_overture_search_with_limit() {
        let args = json!({
            "db_path": "/tmp/spatia.duckdb",
            "table_name": "places_wa",
            "query": "coffee",
            "limit": 5
        });
        let cmd = build_command("overture_search", &args).unwrap();
        assert_eq!(
            cmd,
            "overture_search /tmp/spatia.duckdb places_wa coffee 5"
        );
    }

    #[test]
    fn build_overture_geocode() {
        let args = json!({
            "db_path": "/tmp/spatia.duckdb",
            "table_name": "addresses_ca",
            "query": "321 n lincoln st redlands",
            "limit": 3
        });
        let cmd = build_command("overture_geocode", &args).unwrap();
        assert_eq!(
            cmd,
            "overture_geocode /tmp/spatia.duckdb addresses_ca \"321 n lincoln st redlands\" 3"
        );
    }

    #[test]
    fn build_unknown_tool_errors() {
        let args = json!({});
        let err = build_command("does_not_exist", &args).unwrap_err();
        assert!(err.contains("Unknown tool"));
    }

    #[test]
    fn quote_uses_single_quotes_when_value_has_double_quotes() {
        // String with whitespace + double quote → single-quoted result
        let result = quote("say \"hello\" world").unwrap();
        assert_eq!(result, "'say \"hello\" world'");
    }

    #[test]
    fn quote_errors_on_both_quote_types() {
        let err = quote("it's a \"test\"").unwrap_err();
        assert!(err.contains("both single and double quotes"));
    }

    #[test]
    fn quote_no_whitespace_is_unchanged() {
        assert_eq!(quote("coffee").unwrap(), "coffee");
    }

    // ── handle_line ──────────────────────────────────────────────────────────

    #[test]
    fn handle_initialize_returns_server_info() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let resp = handle_line(line).unwrap();
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["result"]["serverInfo"]["name"], "spatia-mcp");
        assert_eq!(json["result"]["protocolVersion"], "2024-11-05");
    }

    #[test]
    fn handle_ping_returns_empty_result() {
        let line = r#"{"jsonrpc":"2.0","id":42,"method":"ping","params":{}}"#;
        let resp = handle_line(line).unwrap();
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], 42);
        assert_eq!(json["result"], json!({}));
        assert!(json["error"].is_null());
    }

    #[test]
    fn handle_tools_list_contains_all_tools() {
        let line = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
        let resp = handle_line(line).unwrap();
        let json = serde_json::to_value(&resp).unwrap();
        let tools = json["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"ingest_csv"));
        assert!(names.contains(&"get_schema"));
        assert!(names.contains(&"overture_extract"));
        assert!(names.contains(&"overture_search"));
        assert!(names.contains(&"overture_geocode"));
    }

    #[test]
    fn handle_unknown_method_returns_error_32601() {
        let line = r#"{"jsonrpc":"2.0","id":3,"method":"unknown/method","params":{}}"#;
        let resp = handle_line(line).unwrap();
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["error"]["code"], -32601);
        assert!(json["result"].is_null());
    }

    #[test]
    fn handle_notification_returns_none() {
        // Notifications have no `id` field.
        let line = r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#;
        let result = handle_line(line);
        assert!(result.is_none());
    }

    #[test]
    fn handle_invalid_json_returns_parse_error() {
        let resp = handle_line("not valid json {{").unwrap();
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["error"]["code"], -32700);
    }

    #[test]
    fn handle_tools_call_unknown_tool_returns_is_error() {
        let line = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"no_such_tool","arguments":{}}}"#;
        let resp = handle_line(line).unwrap();
        let json = serde_json::to_value(&resp).unwrap();
        // Unknown tool -> error code -32602 (invalid params)
        assert_eq!(json["error"]["code"], -32602);
    }
}
