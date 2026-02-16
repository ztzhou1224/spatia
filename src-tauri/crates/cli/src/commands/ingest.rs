use spatia_engine::ingest_csv_to_table;

pub fn run(db_path: &str, csv_path: &str, table_name: &str) -> EngineResult<()> {
    ingest_csv_to_table(db_path, csv_path, table_name)?;
    println!("ok");
    Ok(())
}

type EngineResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
