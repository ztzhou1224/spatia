pub fn is_help_request(args: &[String]) -> bool {
    matches!(
        args.first().map(String::as_str),
        Some("help" | "-h" | "--help")
    )
}

pub fn print_help() {
    println!("spatia_cli - string-command interface");
    println!();
    println!("usage:");
    println!("  spatia_cli ingest <db_path> <csv_path> [table_name]");
    println!("  spatia_cli schema <db_path> <table_name>");
    println!("  spatia_cli overture_extract <db_path> <theme> <type> <xmin,ymin,xmax,ymax> [table_name]");
    println!("  spatia_cli overture_search <db_path> <table_name> <query> [limit]");
    println!("  spatia_cli overture_geocode <db_path> <addresses_table> <query> [limit]");
    println!("  spatia_cli help");
    println!();
    println!("examples:");
    println!("  spatia_cli ingest ./spatia.duckdb ./data/sample.csv");
    println!("  spatia_cli ingest ./spatia.duckdb ./data/sample.csv places");
    println!("  spatia_cli schema ./spatia.duckdb raw_staging");
    println!("  spatia_cli overture_extract ./spatia.duckdb places place -122.4,47.5,-122.2,47.7 places_wa");
    println!("  spatia_cli overture_search ./spatia.duckdb places_wa \"lincoln\" 10");
    println!("  spatia_cli overture_geocode ./spatia.duckdb addresses_ca \"321 n lincoln st redlands ca 92374\" 5");
}
