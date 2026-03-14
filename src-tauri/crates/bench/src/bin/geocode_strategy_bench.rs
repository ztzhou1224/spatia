//! Benchmark comparing geocoding strategies:
//! - **api_first**: Cache → Geocodio API → GERS reverse (skips Overture S3 downloads)
//! - **overture_first**: Full Overture download cascade → local fuzzy → Geocodio fallback
//!
//! Usage:
//!   cargo run -p spatia_bench --bin geocode_strategy_bench -- --addresses "123 Main St, Springfield, IL" "400 Broad St, Seattle, WA 98109"
//!   cargo run -p spatia_bench --bin geocode_strategy_bench -- --csv /path/to/addresses.csv --column "Property Address"

use std::time::Instant;

use clap::Parser;
use spatia_geocode::{
    components_from_string, geocode_batch_api_first, geocode_batch_overture_first,
    AddressComponents, GeocodeStats,
};

#[derive(Parser, Debug)]
#[command(
    name = "geocode_strategy_bench",
    about = "Compare geocoding strategies: api_first vs overture_first"
)]
struct Cli {
    /// Addresses to geocode (space-separated)
    #[arg(long, num_args = 1..)]
    addresses: Option<Vec<String>>,

    /// CSV file to read addresses from
    #[arg(long)]
    csv: Option<String>,

    /// Column name in CSV containing addresses
    #[arg(long, default_value = "address")]
    column: String,

    /// Only run the api_first strategy
    #[arg(long, default_value_t = false)]
    api_only: bool,

    /// Only run the overture_first strategy
    #[arg(long, default_value_t = false)]
    overture_only: bool,
}

struct BenchResult {
    #[allow(dead_code)]
    strategy: String,
    elapsed_ms: u64,
    stats: GeocodeStats,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let addresses = load_addresses(&cli);
    if addresses.is_empty() {
        eprintln!("ERROR: no addresses provided. Use --addresses or --csv");
        std::process::exit(1);
    }

    println!("geocode_strategy_bench: {} addresses", addresses.len());
    println!();

    let components: Vec<AddressComponents> = addresses
        .iter()
        .map(|a| components_from_string(a))
        .collect();

    // Use a fresh temp DB for each strategy so they don't share cache
    let mut results = Vec::new();

    if !cli.overture_only {
        let db_path = temp_db_path("api_first");
        println!(">>> Running api_first strategy...");
        let start = Instant::now();
        match geocode_batch_api_first(&db_path, &components) {
            Ok((_batch_results, stats)) => {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                println!(
                    "    api_first: {}ms | geocoded={}/{} | cache={} overture_exact={} fuzzy={} api={} unresolved={}",
                    elapsed_ms, stats.geocoded, stats.total,
                    stats.cache_hits, stats.overture_exact, stats.local_fuzzy,
                    stats.api_resolved, stats.unresolved
                );
                results.push(BenchResult {
                    strategy: "api_first".to_string(),
                    elapsed_ms,
                    stats,
                });
            }
            Err(e) => {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                println!("    api_first: FAILED in {}ms — {}", elapsed_ms, e);
            }
        }
        cleanup_db(&db_path);
        println!();
    }

    if !cli.api_only {
        let db_path = temp_db_path("overture_first");
        println!(">>> Running overture_first strategy...");
        let start = Instant::now();
        match geocode_batch_overture_first(&db_path, &components) {
            Ok((_batch_results, stats)) => {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                println!(
                    "    overture_first: {}ms | geocoded={}/{} | cache={} overture_exact={} fuzzy={} api={} unresolved={}",
                    elapsed_ms, stats.geocoded, stats.total,
                    stats.cache_hits, stats.overture_exact, stats.local_fuzzy,
                    stats.api_resolved, stats.unresolved
                );
                results.push(BenchResult {
                    strategy: "overture_first".to_string(),
                    elapsed_ms,
                    stats,
                });
            }
            Err(e) => {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                println!("    overture_first: FAILED in {}ms — {}", elapsed_ms, e);
            }
        }
        cleanup_db(&db_path);
        println!();
    }

    // Print comparison
    if results.len() == 2 {
        println!("========== Strategy Comparison ==========");
        println!();
        println!(
            "  {:20} {:>10} {:>10} {:>10}",
            "Metric", "api_first", "overture_first", "Speedup"
        );
        println!("  {}", "-".repeat(55));

        let api = &results[0];
        let overture = &results[1];

        let speedup = if api.elapsed_ms > 0 {
            overture.elapsed_ms as f64 / api.elapsed_ms as f64
        } else {
            f64::INFINITY
        };

        println!(
            "  {:20} {:>8} ms {:>8} ms {:>9.1}x",
            "Total time", api.elapsed_ms, overture.elapsed_ms, speedup
        );
        println!(
            "  {:20} {:>10} {:>10}",
            "Geocoded", api.stats.geocoded, overture.stats.geocoded
        );
        println!(
            "  {:20} {:>10} {:>10}",
            "Unresolved", api.stats.unresolved, overture.stats.unresolved
        );
        println!(
            "  {:20} {:>10} {:>10}",
            "Cache hits", api.stats.cache_hits, overture.stats.cache_hits
        );
        println!(
            "  {:20} {:>10} {:>10}",
            "Overture exact", api.stats.overture_exact, overture.stats.overture_exact
        );
        println!(
            "  {:20} {:>10} {:>10}",
            "Local fuzzy", api.stats.local_fuzzy, overture.stats.local_fuzzy
        );
        println!(
            "  {:20} {:>10} {:>10}",
            "API resolved", api.stats.api_resolved, overture.stats.api_resolved
        );
        println!();

        if speedup > 1.0 {
            println!(
                "  Result: api_first is {:.1}x faster ({} ms vs {} ms)",
                speedup, api.elapsed_ms, overture.elapsed_ms
            );
        } else {
            println!(
                "  Result: overture_first is {:.1}x faster ({} ms vs {} ms)",
                1.0 / speedup,
                overture.elapsed_ms,
                api.elapsed_ms
            );
        }
        println!();
    }
}

fn load_addresses(cli: &Cli) -> Vec<String> {
    if let Some(addrs) = &cli.addresses {
        return addrs.clone();
    }
    if let Some(csv_path) = &cli.csv {
        return load_csv_column(csv_path, &cli.column);
    }
    Vec::new()
}

fn load_csv_column(path: &str, column: &str) -> Vec<String> {
    let mut rdr = match csv::Reader::from_path(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ERROR: could not open CSV '{}': {}", path, e);
            std::process::exit(1);
        }
    };

    let headers = rdr.headers().expect("CSV headers").clone();
    let col_idx = headers
        .iter()
        .position(|h| h.eq_ignore_ascii_case(column))
        .unwrap_or_else(|| {
            eprintln!(
                "ERROR: column '{}' not found in CSV. Available: {:?}",
                column,
                headers.iter().collect::<Vec<_>>()
            );
            std::process::exit(1);
        });

    let mut addresses = Vec::new();
    for record in rdr.records().flatten() {
        if let Some(val) = record.get(col_idx) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                addresses.push(trimmed.to_string());
            }
        }
    }
    addresses
}

fn temp_db_path(suffix: &str) -> String {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("/tmp/spatia_geocode_strat_bench_{}_{}.duckdb", suffix, ns)
}

fn cleanup_db(path: &str) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}.wal"));
    let _ = std::fs::remove_file(format!("{path}.wal.lck"));
}
