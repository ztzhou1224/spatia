use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::runner::TestResult;

#[derive(Debug, Serialize, Deserialize)]
pub struct BenchReport {
    pub run_at: DateTime<Utc>,
    pub model: String,
    pub corpus_path: String,
    pub summary: Summary,
    pub by_category: HashMap<String, usize>,
    pub results: Vec<TestResult>,
}

#[derive(Debug, Serialize, Deserialize)]
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

/// Load a previously saved JSON benchmark report for comparison.
pub fn load_previous_report(path: &str) -> std::io::Result<BenchReport> {
    let json = std::fs::read_to_string(path)?;
    serde_json::from_str(&json).map_err(std::io::Error::other)
}

/// Print a side-by-side regression comparison between two benchmark runs.
pub fn print_regression_comparison(prev: &BenchReport, current: &BenchReport) {
    println!("========== Regression Comparison ==========");
    println!(
        "  Previous : {} ({}, {})",
        prev.run_at.format("%Y-%m-%d %H:%M"),
        prev.model,
        prev.corpus_path
    );
    println!(
        "  Current  : {} ({}, {})",
        current.run_at.format("%Y-%m-%d %H:%M"),
        current.model,
        current.corpus_path
    );
    println!();

    let ps = &prev.summary;
    let cs = &current.summary;

    let pass_delta = cs.pass_rate_pct - ps.pass_rate_pct;
    let latency_delta = cs.avg_total_ms - ps.avg_total_ms;
    let ai_delta = cs.avg_ai_ms - ps.avg_ai_ms;

    println!("  {:25} {:>10} {:>10} {:>10}", "Metric", "Previous", "Current", "Delta");
    println!("  {}", "-".repeat(55));
    println!(
        "  {:25} {:>9}/{:<4} {:>9}/{:<4} {:>+6}/{:+}",
        "Pass/Total", ps.passed, ps.total, cs.passed, cs.total,
        cs.passed as i64 - ps.passed as i64,
        cs.total as i64 - ps.total as i64
    );
    println!(
        "  {:25} {:>9.1}% {:>10.1}% {:>+10.1}%",
        "Pass rate", ps.pass_rate_pct, cs.pass_rate_pct, pass_delta
    );
    println!(
        "  {:25} {:>9.0} ms {:>9.0} ms {:>+9.0} ms",
        "Avg total latency", ps.avg_total_ms, cs.avg_total_ms, latency_delta
    );
    println!(
        "  {:25} {:>9.0} ms {:>9.0} ms {:>+9.0} ms",
        "Avg AI latency", ps.avg_ai_ms, cs.avg_ai_ms, ai_delta
    );
    println!(
        "  {:25} {:>9.0} ms {:>9.0} ms {:>+9.0} ms",
        "Avg SQL latency", ps.avg_sql_ms, cs.avg_sql_ms, cs.avg_sql_ms - ps.avg_sql_ms
    );
    println!(
        "  {:25} {:>9.2} {:>10.2} {:>+10.2}",
        "Avg round trips", ps.avg_round_trips, cs.avg_round_trips, cs.avg_round_trips - ps.avg_round_trips
    );
    println!(
        "  {:25} {:>8} ms {:>9} ms {:>+9} ms",
        "p50 latency", ps.p50_total_ms, cs.p50_total_ms,
        cs.p50_total_ms as i64 - ps.p50_total_ms as i64
    );
    println!(
        "  {:25} {:>8} ms {:>9} ms {:>+9} ms",
        "p95 latency", ps.p95_total_ms, cs.p95_total_ms,
        cs.p95_total_ms as i64 - ps.p95_total_ms as i64
    );
    println!();

    // Per-test regressions and improvements
    let prev_by_name: HashMap<&str, &TestResult> = prev.results.iter().map(|r| (r.name.as_str(), r)).collect();
    let mut regressions = Vec::new();
    let mut improvements = Vec::new();
    let mut new_tests = Vec::new();

    for cr in &current.results {
        if let Some(pr) = prev_by_name.get(cr.name.as_str()) {
            if pr.outcome == "pass" && cr.outcome != "pass" {
                regressions.push((&cr.name, &cr.outcome, cr.outcome_detail.as_deref()));
            } else if pr.outcome != "pass" && cr.outcome == "pass" {
                improvements.push((&cr.name, &pr.outcome));
            }
        } else {
            new_tests.push((&cr.name, &cr.outcome));
        }
    }

    if !regressions.is_empty() {
        println!("  REGRESSIONS (was passing, now failing):");
        for (name, outcome, detail) in &regressions {
            println!("    [{}] {}", outcome, name);
            if let Some(d) = detail {
                println!("      {}", d);
            }
        }
        println!();
    }

    if !improvements.is_empty() {
        println!("  IMPROVEMENTS (was failing, now passing):");
        for (name, prev_outcome) in &improvements {
            println!("    {} (was {})", name, prev_outcome);
        }
        println!();
    }

    if !new_tests.is_empty() {
        println!("  NEW TESTS:");
        for (name, outcome) in &new_tests {
            println!("    [{}] {}", outcome, name);
        }
        println!();
    }
}
