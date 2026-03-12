mod corpus;
mod report;
mod runner;

use std::path::{Path, PathBuf};

use clap::Parser;
use tracing::{info, warn};

use corpus::Corpus;
use report::BenchReport;
use runner::{run_test, RunnerContext};
use spatia_ai::{GeminiClient, DEFAULT_MODEL};

#[derive(Parser, Debug)]
#[command(
    name = "spatia_bench",
    about = "End-to-end AI analysis benchmark for Spatia"
)]
struct Cli {
    /// Path to the TOML corpus file.
    #[arg(long, default_value = "tests/corpus/analysis_benchmark.toml")]
    corpus: String,

    /// Validate corpus without calling AI.
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Only run tests with these tags (comma-separated).
    #[arg(long)]
    tags: Option<String>,

    /// Gemini model to use.
    #[arg(long, default_value = DEFAULT_MODEL)]
    model: String,

    /// Per-test timeout in seconds.
    #[arg(long, default_value_t = 60)]
    timeout_secs: u64,

    /// Write JSON report to this file.
    #[arg(long)]
    output: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let corpus_path = PathBuf::from(&cli.corpus);
    let corpus_str = std::fs::read_to_string(&corpus_path).unwrap_or_else(|e| {
        eprintln!("ERROR: could not read corpus file '{}': {}", cli.corpus, e);
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

    let test_cases: Vec<_> = corpus
        .filter_by_tags(&tag_filter)
        .into_iter()
        .cloned()
        .collect();

    if test_cases.is_empty() {
        eprintln!("WARNING: no tests matched filter (tags={:?})", tag_filter);
        std::process::exit(0);
    }

    println!("spatia_bench: {} test(s) selected", test_cases.len());

    if cli.dry_run {
        println!("\nDry run -- no AI calls.\n");
        println!("{:<40} {:<15} Description", "Name", "Tags");
        println!("{}", "-".repeat(80));
        for tc in &test_cases {
            println!("{:<40} {:<15} {}", tc.name, tc.tags.join(","), tc.description);
        }
        println!("\nCorpus validated. {} test(s) would run.", test_cases.len());
        return;
    }

    let api_key = std::env::var("SPATIA_GEMINI_API_KEY").unwrap_or_else(|_| {
        eprintln!("ERROR: SPATIA_GEMINI_API_KEY not set. Use --dry-run to validate corpus.");
        std::process::exit(1);
    });
    let client = GeminiClient::with_model(api_key, &cli.model);

    let corpus_dir = corpus_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let ctx = RunnerContext {
        client,
        corpus_dir,
        default_timeout_secs: cli.timeout_secs,
    };

    let total = test_cases.len();
    let mut results = Vec::with_capacity(total);

    for (i, tc) in test_cases.iter().enumerate() {
        print!("  [{:>3}/{:<3}] {:<40} ... ", i + 1, total, tc.name);
        use std::io::Write;
        let _ = std::io::stdout().flush();

        let result = run_test(&ctx, tc).await;
        let status = if result.outcome == "pass" { "PASS" } else { "FAIL" };
        println!(
            "{:<4}  {:>5} ms  {} round-trip(s)",
            status, result.timing.total_ms, result.round_trips
        );
        results.push(result);
    }

    let report = BenchReport::build(results, cli.model.clone(), cli.corpus.clone());
    report.print_summary();

    let output_path = cli.output.clone().unwrap_or_else(|| {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        format!("spatia_bench_report_{ts}.json")
    });

    match report.write_json(&output_path) {
        Ok(()) => info!("JSON report written to {}", output_path),
        Err(e) => warn!("failed to write JSON report: {}", e),
    }

    if report.summary.failed > 0 {
        std::process::exit(1);
    }
}
