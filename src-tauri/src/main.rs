// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // ── DB health-check subprocess mode ──────────────────────────────────────
    // When the main process needs to probe a DuckDB file for corruption it
    // spawns itself with SPATIA_DB_HEALTH_CHECK set to the path to probe.
    // We handle that case here — before any Tauri or GUI initialisation — so
    // that if DuckDB calls abort() it only kills this child process.
    if let Ok(path) = std::env::var("SPATIA_DB_HEALTH_CHECK") {
        let exit_code = run_health_probe(&path);
        std::process::exit(exit_code);
    }

    spatia_lib::run()
}

/// Open the given DuckDB path, run a trivial query, and print the result.
/// Prints "OK" on success, "ERROR:<message>" on failure, then returns the
/// appropriate exit code (0 = ok, 1 = error).
fn run_health_probe(path: &str) -> i32 {
    match duckdb::Connection::open(path) {
        Ok(conn) => match conn.execute_batch("SELECT 1") {
            Ok(_) => {
                println!("OK");
                0
            }
            Err(e) => {
                println!("ERROR:{e}");
                1
            }
        },
        Err(e) => {
            println!("ERROR:{e}");
            1
        }
    }
}
