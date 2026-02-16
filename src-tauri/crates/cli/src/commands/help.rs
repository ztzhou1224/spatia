pub fn is_help_request(args: &[String]) -> bool {
    matches!(
        args.get(0).map(String::as_str),
        Some("help" | "-h" | "--help")
    )
}

pub fn print_help() {
    println!("spatia_cli - data ingestion helper");
    println!();
    println!("usage:");
    println!("  spatia_cli ingest <db_path> <csv_path> <table_name>");
    println!("  spatia_cli help");
    println!();
    println!("example:");
    println!("  spatia_cli ingest ./spatia.duckdb ./data/sample.csv places");
}
