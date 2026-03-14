use std::collections::HashMap;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use duckdb::Connection;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use spatia_geocode::search_index;
use spatia_geocode::{local_fuzzy_geocode, GeocodeBatchResult};

// ── Config ──────────────────────────────────────────────────────────────────

pub struct FuzzyBenchConfig {
    pub ground_truth_csv: String,
    pub variations_csv: String,
}

// ── Ground truth + variation records ────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct GroundTruthRow {
    pub id: String,
    pub label: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VariationRow {
    pub original_id: String,
    pub user_input: String,
    pub variation_type: String,
}

// ── Per-variation result ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzyBenchResult {
    pub original_id: String,
    pub user_input: String,
    pub variation_type: String,
    pub matched: bool,
    pub correct: bool,
    pub matched_label: Option<String>,
    pub distance_m: Option<f64>,
    pub confidence: Option<f64>,
    pub latency_ms: u64,
}

// ── Aggregated report ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuzzyBenchReport {
    pub run_at: String,
    pub total_variations: usize,
    pub match_rate_pct: f64,
    pub correct_match_rate_pct: f64,
    pub wrong_match_rate_pct: f64,
    pub unresolved_rate_pct: f64,
    pub avg_distance_error_m: f64,
    pub p50_distance_m: f64,
    pub p95_distance_m: f64,
    pub avg_confidence_correct: f64,
    pub avg_confidence_wrong: f64,
    pub avg_latency_ms: f64,
    pub by_variation_type: HashMap<String, VariationTypeMetrics>,
    pub results: Vec<FuzzyBenchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariationTypeMetrics {
    pub count: usize,
    pub match_rate_pct: f64,
    pub correct_match_rate_pct: f64,
    pub wrong_match_rate_pct: f64,
    pub unresolved_rate_pct: f64,
    pub avg_distance_error_m: f64,
    pub avg_latency_ms: f64,
}

// ── Haversine ───────────────────────────────────────────────────────────────

fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    r * c
}

fn percentile_f64(sorted: &[f64], pct: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((pct as f64 / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ── Runner ──────────────────────────────────────────────────────────────────

const CORRECT_DISTANCE_THRESHOLD_M: f64 = 50.0;

pub fn run_fuzzy_bench(config: &FuzzyBenchConfig) -> Result<FuzzyBenchReport, Box<dyn std::error::Error + Send + Sync>> {
    // 1. Read ground truth CSV
    let ground_truth = read_ground_truth(&config.ground_truth_csv)?;
    info!(count = ground_truth.len(), "loaded ground truth addresses");

    // 2. Read variations CSV
    let variations = read_variations(&config.variations_csv)?;
    info!(count = variations.len(), "loaded variation test cases");

    // Build ground truth lookup by id
    let gt_by_id: HashMap<&str, &GroundTruthRow> =
        ground_truth.iter().map(|r| (r.id.as_str(), r)).collect();

    // 3. Create temp DuckDB and populate it
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let db_path = format!("/tmp/spatia_fuzzy_bench_{ns}.duckdb");

    let conn = Connection::open(&db_path)?;

    // Create base table with ground truth addresses
    conn.execute_batch(
        "CREATE TABLE fuzzy_bench (id VARCHAR, label TEXT, lat DOUBLE, lon DOUBLE)",
    )?;

    // Create lookup table
    conn.execute_batch(
        "CREATE TABLE fuzzy_bench_lookup (source_id VARCHAR, label TEXT, label_norm TEXT)",
    )?;

    // Insert ground truth data
    {
        let mut insert_base = conn.prepare(
            "INSERT INTO fuzzy_bench VALUES (?, ?, ?, ?)",
        )?;
        let mut insert_lookup = conn.prepare(
            "INSERT INTO fuzzy_bench_lookup VALUES (?, ?, ?)",
        )?;
        for row in &ground_truth {
            insert_base.execute(duckdb::params![row.id, row.label, row.lat, row.lon])?;
            let label_norm = row.label.to_lowercase();
            insert_lookup.execute(duckdb::params![row.id, row.label, label_norm])?;
        }
    }
    info!("populated DuckDB with ground truth data");

    // 4. Build Tantivy index
    let index_dir = search_index::index_dir_for_table(&db_path, "fuzzy_bench_lookup");
    let indexed = search_index::build_index(&conn, "fuzzy_bench_lookup", &index_dir)?;
    info!(indexed, "built Tantivy index for fuzzy_bench_lookup");

    // 5. Run each variation through local_fuzzy_geocode
    let total = variations.len();
    let mut results = Vec::with_capacity(total);

    for (i, var) in variations.iter().enumerate() {
        if (i + 1) % 100 == 0 || i + 1 == total {
            print!("  [{:>4}/{:<4}]\r", i + 1, total);
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }

        let start = Instant::now();
        let addresses = vec![var.user_input.clone()];
        let geocode_results = local_fuzzy_geocode(&conn, &addresses, Some(&db_path));
        let latency_ms = start.elapsed().as_millis() as u64;

        let result = match geocode_results {
            Ok(ref geo_results) => {
                evaluate_result(var, geo_results.first(), &gt_by_id, latency_ms)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    user_input = var.user_input.as_str(),
                    "geocode failed for variation"
                );
                FuzzyBenchResult {
                    original_id: var.original_id.clone(),
                    user_input: var.user_input.clone(),
                    variation_type: var.variation_type.clone(),
                    matched: false,
                    correct: false,
                    matched_label: None,
                    distance_m: None,
                    confidence: None,
                    latency_ms,
                }
            }
        };

        results.push(result);
    }
    println!(); // clear the progress line

    // 6. Cleanup
    drop(conn);
    cleanup_db(&db_path);
    // Clean up the index directory
    let idx_parent = Path::new(&db_path).parent().unwrap_or(Path::new("/tmp"));
    let indexes_dir = idx_parent.join("indexes");
    if indexes_dir.exists() {
        let _ = std::fs::remove_dir_all(&indexes_dir);
    }

    // 7. Build report
    let report = build_report(&results);
    Ok(report)
}

fn evaluate_result(
    var: &VariationRow,
    geo_result: Option<&GeocodeBatchResult>,
    gt_by_id: &HashMap<&str, &GroundTruthRow>,
    latency_ms: u64,
) -> FuzzyBenchResult {
    let Some(result) = geo_result else {
        return FuzzyBenchResult {
            original_id: var.original_id.clone(),
            user_input: var.user_input.clone(),
            variation_type: var.variation_type.clone(),
            matched: false,
            correct: false,
            matched_label: None,
            distance_m: None,
            confidence: None,
            latency_ms,
        };
    };

    let matched = true;
    let matched_label = result.matched_label.clone();
    let confidence = Some(result.confidence);

    // Check correctness by comparing result coordinates to ground truth
    let (correct, distance_m) = if let Some(gt) = gt_by_id.get(var.original_id.as_str()) {
        let dist = haversine_distance(gt.lat, gt.lon, result.lat, result.lon);
        (dist < CORRECT_DISTANCE_THRESHOLD_M, Some(dist))
    } else {
        (false, None)
    };

    FuzzyBenchResult {
        original_id: var.original_id.clone(),
        user_input: var.user_input.clone(),
        variation_type: var.variation_type.clone(),
        matched,
        correct,
        matched_label,
        distance_m,
        confidence,
        latency_ms,
    }
}

fn build_report(results: &[FuzzyBenchResult]) -> FuzzyBenchReport {
    let total = results.len();
    let matched_count = results.iter().filter(|r| r.matched).count();
    let correct_count = results.iter().filter(|r| r.correct).count();
    let wrong_count = matched_count - correct_count;
    let unresolved_count = total - matched_count;

    let match_rate_pct = pct(matched_count, total);
    let correct_match_rate_pct = pct(correct_count, total);
    let wrong_match_rate_pct = pct(wrong_count, total);
    let unresolved_rate_pct = pct(unresolved_count, total);

    // Distance metrics for correct matches
    let mut correct_distances: Vec<f64> = results
        .iter()
        .filter(|r| r.correct)
        .filter_map(|r| r.distance_m)
        .collect();
    correct_distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let avg_distance_error_m = if correct_distances.is_empty() {
        0.0
    } else {
        correct_distances.iter().sum::<f64>() / correct_distances.len() as f64
    };
    let p50_distance_m = percentile_f64(&correct_distances, 50);
    let p95_distance_m = percentile_f64(&correct_distances, 95);

    // Confidence metrics
    let correct_confidences: Vec<f64> = results
        .iter()
        .filter(|r| r.correct)
        .filter_map(|r| r.confidence)
        .collect();
    let avg_confidence_correct = if correct_confidences.is_empty() {
        0.0
    } else {
        correct_confidences.iter().sum::<f64>() / correct_confidences.len() as f64
    };

    let wrong_confidences: Vec<f64> = results
        .iter()
        .filter(|r| r.matched && !r.correct)
        .filter_map(|r| r.confidence)
        .collect();
    let avg_confidence_wrong = if wrong_confidences.is_empty() {
        0.0
    } else {
        wrong_confidences.iter().sum::<f64>() / wrong_confidences.len() as f64
    };

    // Latency
    let avg_latency_ms = if total == 0 {
        0.0
    } else {
        results.iter().map(|r| r.latency_ms as f64).sum::<f64>() / total as f64
    };

    // By variation type
    let mut by_type: HashMap<String, Vec<&FuzzyBenchResult>> = HashMap::new();
    for r in results {
        by_type
            .entry(r.variation_type.clone())
            .or_default()
            .push(r);
    }

    let by_variation_type: HashMap<String, VariationTypeMetrics> = by_type
        .into_iter()
        .map(|(vtype, group)| {
            let n = group.len();
            let m = group.iter().filter(|r| r.matched).count();
            let c = group.iter().filter(|r| r.correct).count();
            let w = m - c;
            let u = n - m;

            let distances: Vec<f64> = group
                .iter()
                .filter(|r| r.correct)
                .filter_map(|r| r.distance_m)
                .collect();
            let avg_dist = if distances.is_empty() {
                0.0
            } else {
                distances.iter().sum::<f64>() / distances.len() as f64
            };

            let avg_lat = if n == 0 {
                0.0
            } else {
                group.iter().map(|r| r.latency_ms as f64).sum::<f64>() / n as f64
            };

            (
                vtype,
                VariationTypeMetrics {
                    count: n,
                    match_rate_pct: pct(m, n),
                    correct_match_rate_pct: pct(c, n),
                    wrong_match_rate_pct: pct(w, n),
                    unresolved_rate_pct: pct(u, n),
                    avg_distance_error_m: avg_dist,
                    avg_latency_ms: avg_lat,
                },
            )
        })
        .collect();

    let run_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();

    FuzzyBenchReport {
        run_at,
        total_variations: total,
        match_rate_pct,
        correct_match_rate_pct,
        wrong_match_rate_pct,
        unresolved_rate_pct,
        avg_distance_error_m,
        p50_distance_m,
        p95_distance_m,
        avg_confidence_correct,
        avg_confidence_wrong,
        avg_latency_ms,
        by_variation_type,
        results: results.to_vec(),
    }
}

fn pct(n: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (n as f64 / total as f64) * 100.0
    }
}

fn cleanup_db(path: &str) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}.wal"));
    let _ = std::fs::remove_file(format!("{path}.wal.lck"));
}

// ── CSV readers ─────────────────────────────────────────────────────────────

fn read_ground_truth(path: &str) -> Result<Vec<GroundTruthRow>, Box<dyn std::error::Error + Send + Sync>> {
    let mut rdr = csv::Reader::from_path(path)?;
    let mut rows = Vec::new();
    for result in rdr.deserialize() {
        let row: GroundTruthRow = result?;
        rows.push(row);
    }
    Ok(rows)
}

fn read_variations(path: &str) -> Result<Vec<VariationRow>, Box<dyn std::error::Error + Send + Sync>> {
    let mut rdr = csv::Reader::from_path(path)?;
    let mut rows = Vec::new();
    for result in rdr.deserialize() {
        let row: VariationRow = result?;
        rows.push(row);
    }
    Ok(rows)
}

// ── Printing ────────────────────────────────────────────────────────────────

impl FuzzyBenchReport {
    pub fn print_summary(&self) {
        println!();
        println!("========== Fuzzy Search Accuracy Benchmark ==========");
        println!("  Run at   : {}", self.run_at);
        println!("  Total    : {} variations", self.total_variations);
        println!();
        println!("  Match rate          : {:.1}%", self.match_rate_pct);
        println!("  Correct match rate  : {:.1}%", self.correct_match_rate_pct);
        println!("  Wrong match rate    : {:.1}%", self.wrong_match_rate_pct);
        println!("  Unresolved rate     : {:.1}%", self.unresolved_rate_pct);
        println!();
        println!("  Distance error (correct matches):");
        println!("    Avg  : {:.1} m", self.avg_distance_error_m);
        println!("    P50  : {:.1} m", self.p50_distance_m);
        println!("    P95  : {:.1} m", self.p95_distance_m);
        println!();
        println!("  Confidence:");
        println!("    Avg (correct) : {:.3}", self.avg_confidence_correct);
        println!("    Avg (wrong)   : {:.3}", self.avg_confidence_wrong);
        println!();
        println!("  Avg latency     : {:.1} ms", self.avg_latency_ms);
        println!();

        if !self.by_variation_type.is_empty() {
            println!("  By variation type:");
            println!(
                "    {:<20} {:>5} {:>8} {:>8} {:>8} {:>8} {:>8}",
                "Type", "Count", "Match%", "Correct%", "Wrong%", "Unres%", "Avg ms"
            );
            println!("    {}", "-".repeat(76));

            let mut types: Vec<(&String, &VariationTypeMetrics)> =
                self.by_variation_type.iter().collect();
            types.sort_by_key(|(k, _)| k.as_str());

            for (vtype, m) in types {
                println!(
                    "    {:<20} {:>5} {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}",
                    vtype,
                    m.count,
                    m.match_rate_pct,
                    m.correct_match_rate_pct,
                    m.wrong_match_rate_pct,
                    m.unresolved_rate_pct,
                    m.avg_latency_ms,
                );
            }
            println!();
        }

        // Show some failures
        let failures: Vec<&FuzzyBenchResult> = self
            .results
            .iter()
            .filter(|r| !r.correct)
            .take(10)
            .collect();
        if !failures.is_empty() {
            println!("  Sample failures (first 10):");
            for f in failures {
                let status = if f.matched { "WRONG" } else { "MISS" };
                println!(
                    "    [{status}] \"{input}\" (id={id}, type={vtype})",
                    status = status,
                    input = f.user_input,
                    id = f.original_id,
                    vtype = f.variation_type,
                );
                if let Some(ref label) = f.matched_label {
                    println!("           matched: \"{}\"", label);
                }
                if let Some(dist) = f.distance_m {
                    println!("           distance: {:.0} m", dist);
                }
            }
            println!();
        }
    }

    pub fn write_json(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }
}
