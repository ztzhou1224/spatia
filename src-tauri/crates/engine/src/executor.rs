use crate::{
    geocode_batch_hybrid, ingest_csv, ingest_csv_to_table, overture_extract_to_table,
    overture_geocode, overture_search, table_schema, BBox, EngineResult,
};

#[derive(Debug, Clone, PartialEq)]
enum Command {
    Ingest {
        db_path: String,
        csv_path: String,
        table_name: Option<String>,
    },
    Schema {
        db_path: String,
        table_name: String,
    },
    Geocode {
        addresses: Vec<String>,
    },
    OvertureExtract {
        db_path: String,
        theme: String,
        item_type: String,
        bbox: BBox,
        table_name: Option<String>,
    },
    OvertureSearch {
        db_path: String,
        table_name: String,
        query: String,
        limit: usize,
    },
    OvertureGeocode {
        db_path: String,
        table_name: String,
        query: String,
        limit: usize,
    },
}

pub fn execute_command(command: &str) -> EngineResult<String> {
    let parsed = parse_command(command)?;
    match parsed {
        Command::Ingest {
            db_path,
            csv_path,
            table_name,
        } => {
            if let Some(table_name) = table_name {
                ingest_csv_to_table(&db_path, &csv_path, &table_name)?;
                Ok(format!(
                    "{{\"status\":\"ok\",\"table\":\"{}\"}}",
                    table_name
                ))
            } else {
                ingest_csv(&db_path, &csv_path)?;
                Ok("{\"status\":\"ok\",\"table\":\"raw_staging\"}".to_string())
            }
        }
        Command::Schema {
            db_path,
            table_name,
        } => {
            let schema = table_schema(&db_path, &table_name)?;
            let json = serde_json::to_string(&schema)?;
            Ok(json)
        }
        Command::Geocode { addresses } => {
            let result = geocode_batch_hybrid(&addresses)?;
            let json = serde_json::to_string(&result)?;
            Ok(json)
        }
        Command::OvertureExtract {
            db_path,
            theme,
            item_type,
            bbox,
            table_name,
        } => {
            let result = overture_extract_to_table(
                &db_path,
                &theme,
                &item_type,
                bbox,
                table_name.as_deref(),
            )?;
            let json = serde_json::to_string(&result)?;
            Ok(json)
        }
        Command::OvertureSearch {
            db_path,
            table_name,
            query,
            limit,
        } => {
            let result = overture_search(&db_path, &table_name, &query, limit)?;
            let json = serde_json::to_string(&result)?;
            Ok(json)
        }
        Command::OvertureGeocode {
            db_path,
            table_name,
            query,
            limit,
        } => {
            let result = overture_geocode(&db_path, &table_name, &query, limit)?;
            let json = serde_json::to_string(&result)?;
            Ok(json)
        }
    }
}

fn parse_command(command: &str) -> EngineResult<Command> {
    let tokens = tokenize(command)?;
    let Some(name) = tokens.first().map(String::as_str) else {
        return Err("Command cannot be empty".into());
    };

    match name {
        "ingest" => parse_ingest(&tokens),
        "schema" => parse_schema(&tokens),
        "geocode" => parse_geocode(&tokens),
        "overture_extract" => parse_overture_extract(&tokens),
        "overture_search" => parse_overture_search(&tokens),
        "overture_geocode" => parse_overture_geocode(&tokens),
        _ => Err(format!("Unknown command: {name}").into()),
    }
}

fn parse_ingest(tokens: &[String]) -> EngineResult<Command> {
    if !(tokens.len() == 3 || tokens.len() == 4) {
        return Err("Usage: ingest <db_path> <csv_path> [table_name]".into());
    }
    let db_path = tokens[1].clone();
    let csv_path = tokens[2].clone();
    let table_name = tokens.get(3).cloned();

    Ok(Command::Ingest {
        db_path,
        csv_path,
        table_name,
    })
}

fn parse_schema(tokens: &[String]) -> EngineResult<Command> {
    if tokens.len() != 3 {
        return Err("Usage: schema <db_path> <table_name>".into());
    }
    Ok(Command::Schema {
        db_path: tokens[1].clone(),
        table_name: tokens[2].clone(),
    })
}

fn parse_geocode(tokens: &[String]) -> EngineResult<Command> {
    if tokens.len() < 2 {
        return Err("Usage: geocode <address_1> <address_2> ...".into());
    }
    let addresses = tokens[1..].to_vec();
    Ok(Command::Geocode { addresses })
}

fn parse_overture_extract(tokens: &[String]) -> EngineResult<Command> {
    if !(tokens.len() == 5 || tokens.len() == 6) {
        return Err(
            "Usage: overture_extract <db_path> <theme> <type> <xmin,ymin,xmax,ymax> [table_name]"
                .into(),
        );
    }
    let bbox = BBox::parse(&tokens[4])?;
    Ok(Command::OvertureExtract {
        db_path: tokens[1].clone(),
        theme: tokens[2].clone(),
        item_type: tokens[3].clone(),
        bbox,
        table_name: tokens.get(5).cloned(),
    })
}

fn parse_overture_search(tokens: &[String]) -> EngineResult<Command> {
    if !(tokens.len() == 4 || tokens.len() == 5) {
        return Err("Usage: overture_search <db_path> <table_name> <query> [limit]".into());
    }

    let limit = if let Some(value) = tokens.get(4) {
        value.parse::<usize>()?
    } else {
        20
    };

    Ok(Command::OvertureSearch {
        db_path: tokens[1].clone(),
        table_name: tokens[2].clone(),
        query: tokens[3].clone(),
        limit,
    })
}

fn parse_overture_geocode(tokens: &[String]) -> EngineResult<Command> {
    if !(tokens.len() == 4 || tokens.len() == 5) {
        return Err("Usage: overture_geocode <db_path> <table_name> <query> [limit]".into());
    }

    let limit = if let Some(value) = tokens.get(4) {
        value.parse::<usize>()?
    } else {
        20
    };

    Ok(Command::OvertureGeocode {
        db_path: tokens[1].clone(),
        table_name: tokens[2].clone(),
        query: tokens[3].clone(),
        limit,
    })
}

fn tokenize(command: &str) -> EngineResult<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in command.chars() {
        match in_quote {
            Some(quote) => {
                if ch == quote {
                    in_quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    in_quote = Some(ch);
                } else if ch.is_whitespace() {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                } else {
                    current.push(ch);
                }
            }
        }
    }

    if in_quote.is_some() {
        return Err("Unterminated quoted string".into());
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::{execute_command, parse_command, Command};
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_ingest_with_optional_table() {
        let command = parse_command("ingest ./db.duckdb ./data.csv places").expect("parse");
        assert_eq!(
            command,
            Command::Ingest {
                db_path: "./db.duckdb".to_string(),
                csv_path: "./data.csv".to_string(),
                table_name: Some("places".to_string()),
            }
        );
    }

    #[test]
    fn parse_ingest_without_table() {
        let command = parse_command("ingest ./db.duckdb ./data.csv").expect("parse");
        assert_eq!(
            command,
            Command::Ingest {
                db_path: "./db.duckdb".to_string(),
                csv_path: "./data.csv".to_string(),
                table_name: None,
            }
        );
    }

    #[test]
    fn parse_geocode_with_quoted_addresses() {
        let command = parse_command("geocode \"San Francisco, CA\" 'New York, NY'").expect("parse");
        assert_eq!(
            command,
            Command::Geocode {
                addresses: vec!["San Francisco, CA".to_string(), "New York, NY".to_string()],
            }
        );
    }

    #[test]
    fn parse_overture_extract_with_bbox() {
        let command = parse_command(
            "overture_extract ./spatia.duckdb places place -122.4,47.5,-122.2,47.7 places_wa",
        )
        .expect("parse");

        match command {
            Command::OvertureExtract {
                db_path,
                theme,
                item_type,
                table_name,
                ..
            } => {
                assert_eq!(db_path, "./spatia.duckdb");
                assert_eq!(theme, "places");
                assert_eq!(item_type, "place");
                assert_eq!(table_name.as_deref(), Some("places_wa"));
            }
            _ => panic!("expected overture extract command"),
        }
    }

    #[test]
    fn parse_overture_search_with_limit() {
        let command = parse_command("overture_search ./spatia.duckdb places_wa \"lincoln\" 5")
            .expect("parse");
        assert_eq!(
            command,
            Command::OvertureSearch {
                db_path: "./spatia.duckdb".to_string(),
                table_name: "places_wa".to_string(),
                query: "lincoln".to_string(),
                limit: 5,
            }
        );
    }

    #[test]
    fn parse_overture_geocode_with_limit() {
        let command = parse_command(
            "overture_geocode ./spatia.duckdb addresses_ca \"321 n lincoln st redlands\" 3",
        )
        .expect("parse");
        assert_eq!(
            command,
            Command::OvertureGeocode {
                db_path: "./spatia.duckdb".to_string(),
                table_name: "addresses_ca".to_string(),
                query: "321 n lincoln st redlands".to_string(),
                limit: 3,
            }
        );
    }

    #[test]
    fn execute_ingest_and_schema_round_trip() {
        let (db_path, csv_path) = setup_files();

        let ingest_cmd = format!("ingest {db_path} {csv_path}");
        let ingest_result = execute_command(&ingest_cmd).expect("ingest execute");
        assert!(ingest_result.contains("raw_staging"));

        let schema_cmd = format!("schema {db_path} raw_staging");
        let schema_result = execute_command(&schema_cmd).expect("schema execute");
        assert!(schema_result.contains("\"name\":\"id\""));
        assert!(schema_result.contains("\"name\":\"city\""));

        cleanup_files(&db_path, &csv_path);
    }

    #[test]
    fn execute_unknown_command_errors() {
        let err = execute_command("unknown").expect_err("should fail");
        assert!(err.to_string().contains("Unknown command"));
    }

    fn setup_files() -> (String, String) {
        let suffix = unique_suffix();
        let db_path = format!("/tmp/spatia_executor_test_{suffix}.duckdb");
        let csv_path = format!("/tmp/spatia_executor_test_{suffix}.csv");
        let mut file = fs::File::create(&csv_path).expect("create csv");
        writeln!(file, "id,city").expect("write header");
        writeln!(file, "1,Oakland").expect("write row");
        (db_path, csv_path)
    }

    fn cleanup_files(db_path: &str, csv_path: &str) {
        let _ = fs::remove_file(db_path);
        let _ = fs::remove_file(format!("{db_path}.wal"));
        let _ = fs::remove_file(format!("{db_path}.wal.lck"));
        let _ = fs::remove_file(csv_path);
    }

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    }
}
