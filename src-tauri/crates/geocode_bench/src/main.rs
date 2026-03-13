mod corpus;
mod report;
mod runner;

use std::path::{Path, PathBuf};

use clap::Parser;
use tracing::info;

use corpus::Corpus;
use report::BenchReport;
use runner::run_test;

#[derive(Parser, Debug)]
#[command(
    name = "spatia_geocode_bench",
    about = "Geocoding benchmark for Spatia"
)]
struct Cli {
    #[arg(long, default_value = "tests/corpus/geocode_benchmark.toml")]
    corpus: String,

    #[arg(long, default_value_t = false)]
    dry_run: bool,

    #[arg(long)]
    tags: Option<String>,

    #[arg(long, default_value_t = 30)]
    timeout_secs: u64,

    #[arg(long)]
    output: Option<String>,

    #[arg(long)]
    compare: Option<String>,

    /// Skip tests tagged with 'requires_api'
    #[arg(long, default_value_t = false)]
    skip_api: bool,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let corpus_path = PathBuf::from(&cli.corpus);
    let corpus_str = std::fs::read_to_string(&corpus_path).unwrap_or_else(|e| {
        eprintln!(
            "ERROR: could not read corpus file '{}': {}",
            cli.corpus, e
        );
        std::process::exit(1);
    });
    let corpus = Corpus::from_str(&corpus_str).unwrap_or_else(|e| {
        eprintln!("ERROR: failed to parse corpus TOML: {}", e);
        std::process::exit(1);
    });

    let tag_filter: Vec<String> = cli
        .tags
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let mut test_cases: Vec<_> = corpus
        .filter_by_tags(&tag_filter)
        .into_iter()
        .cloned()
        .collect();

    // Filter out API tests if --skip-api
    if cli.skip_api {
        test_cases.retain(|tc| !tc.tags.iter().any(|t| t == "requires_api"));
    }

    if test_cases.is_empty() {
        eprintln!(
            "WARNING: no tests matched filter (tags={:?}, skip_api={})",
            tag_filter, cli.skip_api
        );
        std::process::exit(0);
    }

    println!(
        "spatia_geocode_bench: {} test(s) selected",
        test_cases.len()
    );

    if cli.dry_run {
        println!("\nDry run -- no geocoding.\n");
        println!(
            "{:<40} {:<20} Description",
            "Name", "Tags"
        );
        println!("{}", "-".repeat(80));
        for tc in &test_cases {
            println!(
                "{:<40} {:<20} {}",
                tc.name,
                tc.tags.join(","),
                tc.description
            );
        }
        println!(
            "\nCorpus validated. {} test(s) would run.",
            test_cases.len()
        );
        return;
    }

    let corpus_dir = corpus_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let total = test_cases.len();
    let mut results = Vec::with_capacity(total);

    for (i, tc) in test_cases.iter().enumerate() {
        print!("  [{:>3}/{:<3}] {:<40} ... ", i + 1, total, tc.name);
        use std::io::Write;
        let _ = std::io::stdout().flush();

        let result = run_test(tc, &corpus_dir, cli.timeout_secs);
        let status = if result.outcome == "pass" { "PASS" } else { "FAIL" };
        println!("{:<4}  {:>5} ms", status, result.timing.total_ms);
        results.push(result);
    }

    let report = BenchReport::build(results, cli.corpus.clone());
    report.print_summary();

    let output_path = cli.output.clone().unwrap_or_else(|| {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        format!("spatia_geocode_bench_report_{ts}.json")
    });

    match report.write_json(&output_path) {
        Ok(()) => info!("JSON report written to {}", output_path),
        Err(e) => tracing::warn!("failed to write JSON report: {}", e),
    }

    if let Some(compare_path) = &cli.compare {
        match report::load_previous_report(compare_path) {
            Ok(prev) => report::print_regression_comparison(&prev, &report),
            Err(e) => tracing::warn!(
                "failed to load comparison report '{}': {}",
                compare_path,
                e
            ),
        }
    }

    if report.summary.failed > 0 {
        std::process::exit(1);
    }
}
