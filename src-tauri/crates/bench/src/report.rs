use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::runner::TestResult;

#[derive(Debug, Serialize)]
pub struct BenchReport {
    pub run_at: DateTime<Utc>,
    pub model: String,
    pub corpus_path: String,
    pub summary: Summary,
    pub by_category: HashMap<String, usize>,
    pub results: Vec<TestResult>,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate_pct: f64,
    pub avg_total_ms: f64,
    pub avg_ai_ms: f64,
    pub avg_sql_ms: f64,
    pub avg_round_trips: f64,
    pub p50_total_ms: u64,
    pub p95_total_ms: u64,
}

impl BenchReport {
    pub fn build(results: Vec<TestResult>, model: String, corpus_path: String) -> Self {
        let total = results.len();
        let passed = results.iter().filter(|r| r.outcome == "pass").count();
        let failed = total - passed;
        let pass_rate_pct = if total == 0 {
            0.0
        } else {
            (passed as f64 / total as f64) * 100.0
        };

        let avg_total_ms = avg_u64(results.iter().map(|r| r.timing.total_ms));
        let avg_ai_ms = avg_u64(results.iter().map(|r| r.timing.ai_ms));
        let avg_sql_ms = avg_u64(results.iter().map(|r| r.timing.sql_ms));
        let avg_round_trips = avg_usize(results.iter().map(|r| r.round_trips));

        let mut total_ms_sorted: Vec<u64> = results.iter().map(|r| r.timing.total_ms).collect();
        total_ms_sorted.sort_unstable();
        let p50_total_ms = percentile(&total_ms_sorted, 50);
        let p95_total_ms = percentile(&total_ms_sorted, 95);

        let mut by_category: HashMap<String, usize> = HashMap::new();
        for r in &results {
            *by_category.entry(r.outcome.clone()).or_insert(0) += 1;
        }

        BenchReport {
            run_at: Utc::now(),
            model,
            corpus_path,
            summary: Summary {
                total,
                passed,
                failed,
                pass_rate_pct,
                avg_total_ms,
                avg_ai_ms,
                avg_sql_ms,
                avg_round_trips,
                p50_total_ms,
                p95_total_ms,
            },
            by_category,
            results,
        }
    }

    pub fn write_json(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    pub fn print_summary(&self) {
        let s = &self.summary;
        println!();
        println!("========== spatia_bench results ==========");
        println!("  Corpus   : {}", self.corpus_path);
        println!("  Model    : {}", self.model);
        println!("  Run at   : {}", self.run_at.format("%Y-%m-%d %H:%M:%S UTC"));
        println!();
        println!("  Total    : {}", s.total);
        println!("  Passed   : {} ({:.1}%)", s.passed, s.pass_rate_pct);
        println!("  Failed   : {}", s.failed);
        println!();

        if !self.by_category.is_empty() {
            println!("  Outcome breakdown:");
            let mut cats: Vec<(&String, &usize)> = self.by_category.iter().collect();
            cats.sort_by_key(|(k, _)| k.as_str());
            for (cat, count) in cats {
                let pct = if s.total == 0 {
                    0.0
                } else {
                    (*count as f64 / s.total as f64) * 100.0
                };
                println!("    {:<25} {:>4}  ({:.1}%)", cat, count, pct);
            }
            println!();
        }

        println!("  Latency (avg):");
        println!("    Total  : {:.0} ms", s.avg_total_ms);
        println!("    AI     : {:.0} ms", s.avg_ai_ms);
        println!("    SQL    : {:.0} ms", s.avg_sql_ms);
        println!("    p50    : {} ms", s.p50_total_ms);
        println!("    p95    : {} ms", s.p95_total_ms);
        println!();
        println!("  Avg round trips: {:.2}", s.avg_round_trips);
        println!();

        let failures: Vec<&TestResult> = self
            .results
            .iter()
            .filter(|r| r.outcome != "pass")
            .collect();
        if !failures.is_empty() {
            println!("  Failed tests:");
            for r in failures {
                println!("    [{}] {} - {}", r.outcome, r.name, r.description);
                if let Some(detail) = &r.outcome_detail {
                    let truncated: String = detail.chars().take(200).collect();
                    println!("      {}", truncated);
                }
            }
            println!();
        }
    }
}

fn avg_u64(iter: impl Iterator<Item = u64>) -> f64 {
    let values: Vec<u64> = iter.collect();
    if values.is_empty() { return 0.0; }
    values.iter().sum::<u64>() as f64 / values.len() as f64
}

fn avg_usize(iter: impl Iterator<Item = usize>) -> f64 {
    let values: Vec<usize> = iter.collect();
    if values.is_empty() { return 0.0; }
    values.iter().sum::<usize>() as f64 / values.len() as f64
}

fn percentile(sorted: &[u64], pct: usize) -> u64 {
    if sorted.is_empty() { return 0; }
    let idx = ((pct as f64 / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}
