use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::runner::TestResult;

#[derive(Debug, Serialize, Deserialize)]
pub struct BenchReport {
    pub run_at: DateTime<Utc>,
    pub corpus_path: String,
    pub summary: Summary,
    pub by_category: HashMap<String, usize>,
    pub source_distribution: SourceDistribution,
    pub results: Vec<TestResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Summary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate_pct: f64,
    pub avg_total_ms: f64,
    pub avg_geocode_ms: f64,
    pub avg_setup_ms: f64,
    pub p50_total_ms: u64,
    pub p95_total_ms: u64,
    pub avg_distance_error_m: f64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SourceDistribution {
    pub cache: usize,
    pub local_fuzzy: usize,
    pub api: usize,
    pub unresolved: usize,
}

impl BenchReport {
    pub fn build(results: Vec<TestResult>, corpus_path: String) -> Self {
        let total = results.len();
        let passed = results.iter().filter(|r| r.outcome == "pass").count();
        let failed = total - passed;
        let pass_rate_pct = if total == 0 {
            0.0
        } else {
            (passed as f64 / total as f64) * 100.0
        };

        let avg_total_ms = avg_u64(results.iter().map(|r| r.timing.total_ms));
        let avg_geocode_ms = avg_u64(results.iter().map(|r| r.timing.geocode_ms));
        let avg_setup_ms = avg_u64(results.iter().map(|r| r.timing.setup_ms));

        let mut total_ms_sorted: Vec<u64> = results.iter().map(|r| r.timing.total_ms).collect();
        total_ms_sorted.sort_unstable();
        let p50_total_ms = percentile(&total_ms_sorted, 50);
        let p95_total_ms = percentile(&total_ms_sorted, 95);

        // Avg distance error from all address results
        let all_distances: Vec<f64> = results
            .iter()
            .flat_map(|r| r.address_results.iter())
            .filter_map(|ar| ar.distance_error_m)
            .collect();
        let avg_distance_error_m = if all_distances.is_empty() {
            0.0
        } else {
            all_distances.iter().sum::<f64>() / all_distances.len() as f64
        };

        // Source distribution
        let mut source_dist = SourceDistribution::default();
        for r in &results {
            if let Some(stats) = &r.stats {
                source_dist.cache += stats.cache_hits;
                source_dist.local_fuzzy += stats.local_fuzzy;
                source_dist.api += stats.api_resolved;
                source_dist.unresolved += stats.unresolved;
            }
        }

        let mut by_category: HashMap<String, usize> = HashMap::new();
        for r in &results {
            *by_category.entry(r.outcome.clone()).or_insert(0) += 1;
        }

        BenchReport {
            run_at: Utc::now(),
            corpus_path,
            summary: Summary {
                total,
                passed,
                failed,
                pass_rate_pct,
                avg_total_ms,
                avg_geocode_ms,
                avg_setup_ms,
                p50_total_ms,
                p95_total_ms,
                avg_distance_error_m,
            },
            by_category,
            source_distribution: source_dist,
            results,
        }
    }

    pub fn write_json(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    pub fn print_summary(&self) {
        let s = &self.summary;
        println!();
        println!("========== spatia_geocode_bench results ==========");
        println!("  Corpus   : {}", self.corpus_path);
        println!(
            "  Run at   : {}",
            self.run_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
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

        let sd = &self.source_distribution;
        let total_addresses = sd.cache + sd.local_fuzzy + sd.api + sd.unresolved;
        if total_addresses > 0 {
            println!(
                "  Source distribution ({} addresses):",
                total_addresses
            );
            println!(
                "    Cache        : {:>4}  ({:.1}%)",
                sd.cache,
                pct(sd.cache, total_addresses)
            );
            println!(
                "    Local fuzzy  : {:>4}  ({:.1}%)",
                sd.local_fuzzy,
                pct(sd.local_fuzzy, total_addresses)
            );
            println!(
                "    API          : {:>4}  ({:.1}%)",
                sd.api,
                pct(sd.api, total_addresses)
            );
            println!(
                "    Unresolved   : {:>4}  ({:.1}%)",
                sd.unresolved,
                pct(sd.unresolved, total_addresses)
            );
            println!();
        }

        println!("  Latency:");
        println!("    Total avg  : {:.0} ms", s.avg_total_ms);
        println!("    Geocode avg: {:.0} ms", s.avg_geocode_ms);
        println!("    Setup avg  : {:.0} ms", s.avg_setup_ms);
        println!("    p50        : {} ms", s.p50_total_ms);
        println!("    p95        : {} ms", s.p95_total_ms);
        println!();
        println!("  Avg distance error: {:.1} m", s.avg_distance_error_m);
        println!();

        let failures: Vec<&TestResult> = self
            .results
            .iter()
            .filter(|r| r.outcome != "pass")
            .collect();
        if !failures.is_empty() {
            println!("  Failed tests:");
            for r in failures {
                println!(
                    "    [{}] {} - {}",
                    r.outcome, r.name, r.description
                );
                if let Some(detail) = &r.outcome_detail {
                    let truncated: String = detail.chars().take(200).collect();
                    println!("      {}", truncated);
                }
            }
            println!();
        }
    }
}

fn pct(n: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (n as f64 / total as f64) * 100.0
    }
}

fn avg_u64(iter: impl Iterator<Item = u64>) -> f64 {
    let values: Vec<u64> = iter.collect();
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<u64>() as f64 / values.len() as f64
}

fn percentile(sorted: &[u64], pct: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx =
        ((pct as f64 / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

pub fn load_previous_report(path: &str) -> std::io::Result<BenchReport> {
    let json = std::fs::read_to_string(path)?;
    serde_json::from_str(&json).map_err(std::io::Error::other)
}

pub fn print_regression_comparison(prev: &BenchReport, current: &BenchReport) {
    println!("========== Regression Comparison ==========");
    println!(
        "  Previous : {} ({})",
        prev.run_at.format("%Y-%m-%d %H:%M"),
        prev.corpus_path
    );
    println!(
        "  Current  : {} ({})",
        current.run_at.format("%Y-%m-%d %H:%M"),
        current.corpus_path
    );
    println!();

    let ps = &prev.summary;
    let cs = &current.summary;

    println!(
        "  {:25} {:>10} {:>10} {:>10}",
        "Metric", "Previous", "Current", "Delta"
    );
    println!("  {}", "-".repeat(55));
    println!(
        "  {:25} {:>9.1}% {:>10.1}% {:>+10.1}%",
        "Pass rate",
        ps.pass_rate_pct,
        cs.pass_rate_pct,
        cs.pass_rate_pct - ps.pass_rate_pct
    );
    println!(
        "  {:25} {:>9.0} ms {:>9.0} ms {:>+9.0} ms",
        "Avg total latency",
        ps.avg_total_ms,
        cs.avg_total_ms,
        cs.avg_total_ms - ps.avg_total_ms
    );
    println!(
        "  {:25} {:>9.0} ms {:>9.0} ms {:>+9.0} ms",
        "Avg geocode latency",
        ps.avg_geocode_ms,
        cs.avg_geocode_ms,
        cs.avg_geocode_ms - ps.avg_geocode_ms
    );
    println!(
        "  {:25} {:>9.1} m {:>10.1} m {:>+10.1} m",
        "Avg distance error",
        ps.avg_distance_error_m,
        cs.avg_distance_error_m,
        cs.avg_distance_error_m - ps.avg_distance_error_m
    );
    println!();

    // Per-test comparison
    let prev_by_name: HashMap<&str, &TestResult> =
        prev.results.iter().map(|r| (r.name.as_str(), r)).collect();
    let mut regressions = Vec::new();
    let mut improvements = Vec::new();

    for cr in &current.results {
        if let Some(pr) = prev_by_name.get(cr.name.as_str()) {
            if pr.outcome == "pass" && cr.outcome != "pass" {
                regressions.push((&cr.name, &cr.outcome, cr.outcome_detail.as_deref()));
            } else if pr.outcome != "pass" && cr.outcome == "pass" {
                improvements.push((&cr.name, &pr.outcome));
            }
        }
    }

    if !regressions.is_empty() {
        println!("  REGRESSIONS:");
        for (name, outcome, detail) in &regressions {
            println!("    [{}] {}", outcome, name);
            if let Some(d) = detail {
                println!("      {}", d);
            }
        }
        println!();
    }

    if !improvements.is_empty() {
        println!("  IMPROVEMENTS:");
        for (name, prev_outcome) in &improvements {
            println!("    {} (was {})", name, prev_outcome);
        }
        println!();
    }
}
