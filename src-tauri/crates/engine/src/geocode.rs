use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::thread;
use std::time::{Duration, Instant};

use crate::geocode_cache::{cache_lookup, cache_store, ensure_cache_table};
use crate::geocodio::geocode_via_geocodio;
use crate::EngineResult;

pub const DEFAULT_GEOCODER_URL: &str = "http://127.0.0.1:7788";
const DEFAULT_DAEMON_THRESHOLD: usize = 100;

#[derive(Debug, Clone, Serialize)]
struct GeocodeRequest {
    addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeocodeResult {
    pub address: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeocodeMode {
    OneShot,
    Daemon,
    Auto,
}

enum GeocoderRunner {
    Binary(PathBuf),
    PythonScript(PathBuf),
}

impl FromStr for GeocodeMode {
    type Err = Box<dyn std::error::Error + Send + Sync>;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "oneshot" => Ok(Self::OneShot),
            "daemon" => Ok(Self::Daemon),
            "auto" => Ok(Self::Auto),
            _ => Err(format!(
                "Invalid SPATIA_GEOCODER_MODE '{value}'. Expected one of: oneshot, daemon, auto"
            )
            .into()),
        }
    }
}

/// Geocode a batch of addresses using a cache-first strategy:
///
/// 1. If `db_path` is `Some`, open the DuckDB file and return cached results
///    for any addresses already in `geocode_cache`.
/// 2. Send the remaining addresses to the local Python sidecar (geopy).
/// 3. For addresses the sidecar could not resolve, call the Geocodio API as a
///    fallback (requires `SPATIA_GEOCODIO_API_KEY`).
/// 4. Write all newly resolved results back to the cache.
pub fn geocode_batch_hybrid(
    addresses: &[String],
    db_path: Option<&str>,
) -> EngineResult<Vec<GeocodeResult>> {
    // --- Step 1: cache lookup ---
    let (mut resolved, to_geocode) = if let Some(path) = db_path {
        let conn = duckdb::Connection::open(path)?;
        ensure_cache_table(&conn)?;
        let (hits, misses) = cache_lookup(&conn, addresses)?;
        (hits, misses)
    } else {
        (vec![], addresses.to_vec())
    };

    if to_geocode.is_empty() {
        return Ok(resolved);
    }

    // --- Step 2: sidecar geocoding ---
    let mode = geocode_mode_from_env()?;
    let sidecar_results = match mode {
        GeocodeMode::OneShot => geocode_batch_oneshot(&to_geocode),
        GeocodeMode::Daemon => geocode_batch_daemon(DEFAULT_GEOCODER_URL, &to_geocode),
        GeocodeMode::Auto => {
            if should_use_daemon(to_geocode.len()) {
                geocode_batch_daemon(DEFAULT_GEOCODER_URL, &to_geocode)
            } else {
                match geocode_batch_oneshot(&to_geocode) {
                    Ok(results) => Ok(results),
                    Err(_) => geocode_batch_daemon(DEFAULT_GEOCODER_URL, &to_geocode),
                }
            }
        }
    };

    // Sidecar errors are intentionally swallowed: a failed sidecar is treated as
    // "zero results" so that the Geocodio fallback path can handle all addresses.
    let sidecar_resolved = sidecar_results.unwrap_or_default();

    // Persist sidecar hits to cache.  Cache failures are non-fatal.
    if let Some(path) = db_path {
        if !sidecar_resolved.is_empty() {
            if let Ok(conn) = duckdb::Connection::open(path) {
                let _ = cache_store(&conn, &sidecar_resolved, "sidecar");
            }
        }
    }

    // Collect addresses the sidecar resolved vs. those still missing coords.
    let sidecar_hit_set: std::collections::HashSet<&str> = sidecar_resolved
        .iter()
        .filter(|r| r.lat.is_some() && r.lon.is_some())
        .map(|r| r.address.as_str())
        .collect();

    let still_missing: Vec<String> = to_geocode
        .iter()
        .filter(|a| !sidecar_hit_set.contains(a.as_str()))
        .cloned()
        .collect();

    resolved.extend(sidecar_resolved);

    // --- Step 3: Geocodio fallback ---
    if !still_missing.is_empty() && std::env::var("SPATIA_GEOCODIO_API_KEY").is_ok() {
        // Geocodio errors are intentionally swallowed: callers receive null-coord
        // entries rather than a hard error when the external service is unavailable.
        let geocodio_results = geocode_via_geocodio(&still_missing).unwrap_or_default();

        if let Some(path) = db_path {
            // Cache failures are non-fatal.
            if !geocodio_results.is_empty() {
                if let Ok(conn) = duckdb::Connection::open(path) {
                    let _ = cache_store(&conn, &geocodio_results, "geocodio");
                }
            }
        }

        resolved.extend(geocodio_results);
    } else if !still_missing.is_empty() {
        // No Geocodio key — propagate unresolved addresses as null-coord entries.
        for address in still_missing {
            resolved.push(GeocodeResult {
                address,
                lat: None,
                lon: None,
                status: None,
                error: None,
            });
        }
    }

    Ok(resolved)
}

pub async fn geocode_batch(
    base_url: &str,
    addresses: &[String],
) -> EngineResult<Vec<GeocodeResult>> {
    let trimmed = base_url.trim_end_matches('/');
    let url = format!("{trimmed}/geocode");
    let client = reqwest::Client::new();
    let payload = GeocodeRequest {
        addresses: addresses.to_vec(),
    };
    let response = client.post(url).json(&payload).send().await?;
    let response = response.error_for_status()?;
    let results = response.json::<Vec<GeocodeResult>>().await?;
    Ok(results)
}

fn geocode_batch_oneshot(addresses: &[String]) -> EngineResult<Vec<GeocodeResult>> {
    let runner = resolve_geocoder_runner()?;
    let output = match runner {
        GeocoderRunner::Binary(path) => Command::new(path).args(addresses).output()?,
        GeocoderRunner::PythonScript(path) => {
            let interpreter = std::env::var("SPATIA_GEOCODER_PYTHON").unwrap_or_else(|_| {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let candidates = [
                    cwd.join("../.venv/bin/python"),
                    cwd.join(".venv/bin/python"),
                    cwd.join("../../.venv/bin/python"),
                ];
                candidates
                    .into_iter()
                    .find(|candidate| candidate.is_file())
                    .map(|candidate| candidate.to_string_lossy().to_string())
                    .unwrap_or_else(|| "python3".to_string())
            });
            Command::new(interpreter)
                .arg(path)
                .args(addresses)
                .output()?
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "geocoder one-shot process failed with status {}: {}",
            output.status,
            stderr.trim()
        )
        .into());
    }

    if output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "geocoder one-shot process returned empty output. Rebuild sidecar binary or use SPATIA_GEOCODER_BIN/SPATIA_GEOCODER_PYTHON. stderr: {}",
            stderr.trim()
        )
        .into());
    }

    let parsed = serde_json::from_slice::<Vec<GeocodeResult>>(&output.stdout)?;
    Ok(parsed)
}

fn geocode_batch_daemon(base_url: &str, addresses: &[String]) -> EngineResult<Vec<GeocodeResult>> {
    let mut spawned = ensure_daemon_running(base_url)?;
    let result = run_http_geocode(base_url, addresses);

    if let Some(child) = spawned.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }

    result
}

fn run_http_geocode(base_url: &str, addresses: &[String]) -> EngineResult<Vec<GeocodeResult>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(geocode_batch(base_url, addresses))
}

fn ensure_daemon_running(base_url: &str) -> EngineResult<Option<Child>> {
    if geocoder_health_check(base_url) {
        return Ok(None);
    }

    let binary_path = resolve_geocoder_binary_path()?;
    let mut child = Command::new(&binary_path)
        .arg("--serve")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    if wait_for_daemon(base_url, Duration::from_secs(5)) {
        return Ok(Some(child));
    }

    let _ = child.kill();
    let _ = child.wait();

    let mut child = Command::new(binary_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    if wait_for_daemon(base_url, Duration::from_secs(5)) {
        return Ok(Some(child));
    }

    let _ = child.kill();
    let _ = child.wait();

    Err("Timed out waiting for geocoder daemon readiness".into())
}

fn geocoder_health_check(base_url: &str) -> bool {
    let probe_payload = Vec::new();
    run_http_geocode(base_url, &probe_payload).is_ok()
}

fn wait_for_daemon(base_url: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if geocoder_health_check(base_url) {
            return true;
        }
        thread::sleep(Duration::from_millis(150));
    }
    false
}

fn geocode_mode_from_env() -> EngineResult<GeocodeMode> {
    match std::env::var("SPATIA_GEOCODER_MODE") {
        Ok(value) => value.parse(),
        Err(_) => Ok(GeocodeMode::Auto),
    }
}

fn daemon_threshold() -> usize {
    std::env::var("SPATIA_GEOCODER_DAEMON_THRESHOLD")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_DAEMON_THRESHOLD)
}

fn should_use_daemon(batch_size: usize) -> bool {
    batch_size >= daemon_threshold()
}

fn resolve_geocoder_binary_path() -> EngineResult<PathBuf> {
    if let Ok(explicit) = std::env::var("SPATIA_GEOCODER_BIN") {
        let explicit_path = PathBuf::from(explicit);
        if !explicit_path.is_file() {
            return Err("SPATIA_GEOCODER_BIN is set but does not point to a file".into());
        }
        if explicit_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("py"))
            .unwrap_or(false)
        {
            return Err(
                "Daemon mode requires a packaged geocoder binary, not a Python source file".into(),
            );
        }
        return Ok(explicit_path);
    }

    let current_dir = std::env::current_dir()?;
    let host = host_triple();
    let binary_candidates = vec![
        current_dir.join("../src-python/spatia-geocoder/dist/main"),
        current_dir.join("src-python/spatia-geocoder/dist/main"),
        current_dir.join(format!("binaries/spatia-geocoder-{host}")),
        current_dir.join(format!("src-tauri/binaries/spatia-geocoder-{host}")),
        current_dir.join(format!("../src-tauri/binaries/spatia-geocoder-{host}")),
    ];

    for candidate in binary_candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err("Daemon mode requires a packaged geocoder binary. Set SPATIA_GEOCODER_BIN to a built executable.".into())
}

fn resolve_geocoder_runner() -> EngineResult<GeocoderRunner> {
    if let Ok(explicit) = std::env::var("SPATIA_GEOCODER_BIN") {
        let explicit_path = PathBuf::from(explicit);
        if explicit_path.is_file() {
            if explicit_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("py"))
                .unwrap_or(false)
            {
                return Ok(GeocoderRunner::PythonScript(explicit_path));
            }
            return Ok(GeocoderRunner::Binary(explicit_path));
        }
        return Err("SPATIA_GEOCODER_BIN is set but does not point to a file".into());
    }

    let current_dir = std::env::current_dir()?;
    let host = host_triple();
    let script_candidates = vec![
        current_dir.join("../src-python/spatia-geocoder/main.py"),
        current_dir.join("src-python/spatia-geocoder/main.py"),
    ];
    for candidate in script_candidates {
        if candidate.is_file() {
            return Ok(GeocoderRunner::PythonScript(candidate));
        }
    }

    let binary_candidates = vec![
        current_dir.join("../src-python/spatia-geocoder/dist/main"),
        current_dir.join("src-python/spatia-geocoder/dist/main"),
        current_dir.join(format!("binaries/spatia-geocoder-{host}")),
        current_dir.join(format!("src-tauri/binaries/spatia-geocoder-{host}")),
        current_dir.join(format!("../src-tauri/binaries/spatia-geocoder-{host}")),
    ];

    for candidate in binary_candidates {
        if candidate.is_file() {
            return Ok(GeocoderRunner::Binary(candidate));
        }
    }

    Err(
        "Unable to locate geocoder runner. Set SPATIA_GEOCODER_BIN or run package_sidecar.sh"
            .into(),
    )
}

fn host_triple() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        _ => "aarch64-apple-darwin",
    }
}

#[cfg(test)]
mod tests {
    use super::{should_use_daemon, GeocodeMode};

    #[test]
    fn parse_geocode_mode_values() {
        assert_eq!(
            "oneshot".parse::<GeocodeMode>().expect("parse"),
            GeocodeMode::OneShot
        );
        assert_eq!(
            "daemon".parse::<GeocodeMode>().expect("parse"),
            GeocodeMode::Daemon
        );
        assert_eq!(
            "auto".parse::<GeocodeMode>().expect("parse"),
            GeocodeMode::Auto
        );
    }

    #[test]
    fn daemon_threshold_selector() {
        std::env::set_var("SPATIA_GEOCODER_DAEMON_THRESHOLD", "3");
        assert!(!should_use_daemon(2));
        assert!(should_use_daemon(3));
        std::env::remove_var("SPATIA_GEOCODER_DAEMON_THRESHOLD");
    }
}
