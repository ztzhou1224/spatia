use std::env;
use std::io::{self, Read};

mod commands;
use spatia_engine::execute_command;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut args = env::args().skip(1).collect::<Vec<String>>();
    if args.is_empty() {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        args = input.split_whitespace().map(str::to_string).collect();
    }

    if args.is_empty() || commands::help::is_help_request(&args) {
        commands::help::print_help();
        return Ok(());
    }

    if !matches!(
        args[0].as_str(),
        "ingest"
            | "schema"
            | "geocode"
            | "overture_extract"
            | "overture_search"
            | "overture_geocode"
    ) {
        commands::help::print_help();
        return Ok(());
    }

    let command = serialize_command(&args);
    let output = execute_command(&command)?;
    println!("{output}");

    Ok(())
}

fn serialize_command(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.chars().any(char::is_whitespace) {
                format!("\"{}\"", arg.replace('"', "\\\""))
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}
