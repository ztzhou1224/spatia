use std::env;
use std::io::{self, Read};

mod commands;

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
        args = input
            .split_whitespace()
            .map(|value| value.to_string())
            .collect();
    }

    if args.is_empty() || commands::help::is_help_request(&args) {
        commands::help::print_help();
        return Ok(());
    }

    match args[0].as_str() {
        "ingest" => {
            if args.len() != 4 {
                commands::help::print_help();
                return Ok(());
            }
            commands::ingest::run(&args[1], &args[2], &args[3])?;
        }
        _ => {
            commands::help::print_help();
        }
    }
    Ok(())
}
