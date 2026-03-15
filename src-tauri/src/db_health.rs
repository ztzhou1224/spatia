//! DuckDB file health check and recovery helpers.
//!
//! The primary concern is silent corruption that causes DuckDB's C++ layer to
//! call `abort()` (e.g. in `single_file_block_manager.cpp`).  A normal Rust
//! `Result` cannot catch an `abort()`, so we detect corruption by spawning a
//! *child process* that attempts to open the database.  If that child exits
//! abnormally (signal / non-zero) or prints an error, the file is corrupt.

use std::path::Path;
use tracing::{error, info, warn};

/// Magic bytes that identify a DuckDB database file.
/// In DuckDB 1.x the header is: 8 bytes (checksum/version) followed by "DUCK".
/// We check bytes 8..12.
const DUCKDB_MAGIC: &[u8] = b"DUCK";
const DUCKDB_MAGIC_OFFSET: usize = 8;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status")]
pub enum DbHealthStatus {
    Healthy {
        size_bytes: u64,
        table_count: usize,
    },
    Corrupt {
        error: String,
        file_size: u64,
    },
    Missing,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum RecoveryAction {
    BackupAndRecreate,
    DeleteAndRecreate,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecoveryResult {
    pub success: bool,
    pub message: String,
    pub backup_path: Option<String>,
}

/// Check the health of the DuckDB file at `db_path`.
///
/// Steps:
/// 1. If the file does not exist → `Missing`.
/// 2. Read the first 4 bytes and verify the DuckDB magic header.  A truncated
///    or zero-length file is treated as corrupt immediately without spawning a
///    child.
/// 3. Spawn the current executable with `SPATIA_DB_HEALTH_CHECK=<path>` so the
///    probe runs in an isolated process.  DuckDB's `abort()` on corruption only
///    kills the child, not the main app.
/// 4. If the child exits cleanly and prints "OK", return `Healthy`.
///    Otherwise return `Corrupt` with the captured error text.
pub fn check_db_health(db_path: &str) -> DbHealthStatus {
    let path = Path::new(db_path);

    // --- Step 1: existence check ---
    if !path.exists() {
        info!(db_path, "db_health: file not found → Missing");
        return DbHealthStatus::Missing;
    }

    // --- Step 2: magic-byte check ---
    let file_size = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) => {
            warn!(db_path, error = %e, "db_health: cannot stat file → Corrupt");
            return DbHealthStatus::Corrupt {
                error: format!("cannot stat file: {e}"),
                file_size: 0,
            };
        }
    };

    let min_header_size = (DUCKDB_MAGIC_OFFSET + DUCKDB_MAGIC.len()) as u64;
    if file_size < min_header_size {
        warn!(db_path, file_size, "db_health: file too small → Corrupt");
        return DbHealthStatus::Corrupt {
            error: format!("file too small ({file_size} bytes) to be a valid DuckDB database"),
            file_size,
        };
    }

    let mut buf = vec![0u8; DUCKDB_MAGIC_OFFSET + DUCKDB_MAGIC.len()];
    if let Ok(mut f) = std::fs::File::open(path) {
        use std::io::Read;
        if f.read_exact(&mut buf).is_err() {
            warn!(db_path, "db_health: cannot read header → Corrupt");
            return DbHealthStatus::Corrupt {
                error: "cannot read file header".to_string(),
                file_size,
            };
        }
    }

    let header = &buf[DUCKDB_MAGIC_OFFSET..DUCKDB_MAGIC_OFFSET + DUCKDB_MAGIC.len()];
    if header != DUCKDB_MAGIC {
        warn!(
            db_path,
            header = ?header,
            "db_health: bad magic bytes → Corrupt"
        );
        return DbHealthStatus::Corrupt {
            error: format!(
                "invalid DuckDB magic bytes: {:02X?} (expected {:02X?})",
                header, DUCKDB_MAGIC
            ),
            file_size,
        };
    }

    // --- Step 3: subprocess probe ---
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            // Cannot determine current exe; fall back to "assume healthy" so we
            // don't block startup when the subprocess approach isn't available.
            warn!(
                db_path,
                error = %e,
                "db_health: cannot determine current exe; skipping subprocess probe"
            );
            return DbHealthStatus::Healthy {
                size_bytes: file_size,
                table_count: 0,
            };
        }
    };

    info!(db_path, exe = %exe.display(), "db_health: spawning subprocess probe");

    let output = std::process::Command::new(&exe)
        .env("SPATIA_DB_HEALTH_CHECK", db_path)
        // Suppress any Tauri / GUI initialisation in the child.  The child
        // exits before touching any Tauri surface area.
        .output();

    match output {
        Err(e) => {
            error!(db_path, error = %e, "db_health: failed to spawn probe process");
            // Conservative: if we can't spawn, assume healthy to avoid blocking.
            DbHealthStatus::Healthy {
                size_bytes: file_size,
                table_count: 0,
            }
        }
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stdout = stdout.trim();

            if out.status.success() && stdout == "OK" {
                info!(db_path, "db_health: probe succeeded → Healthy");
                // Parse table count from "OK:<n>" if the probe emits it,
                // otherwise leave at 0 (count is informational only).
                DbHealthStatus::Healthy {
                    size_bytes: file_size,
                    table_count: 0,
                }
            } else {
                let error_msg = if let Some(rest) = stdout.strip_prefix("ERROR:") {
                    rest.to_string()
                } else if !stdout.is_empty() {
                    stdout.to_string()
                } else {
                    // No stdout — check stderr or exit code
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if stderr.trim().is_empty() {
                        format!(
                            "probe exited with status {} and no output (likely abort())",
                            out.status
                        )
                    } else {
                        stderr.trim().to_string()
                    }
                };
                error!(db_path, error = %error_msg, "db_health: probe failed → Corrupt");
                DbHealthStatus::Corrupt {
                    error: error_msg,
                    file_size,
                }
            }
        }
    }
}

/// Recover a corrupt DuckDB database.
///
/// Both actions rename/remove the main file and the two DuckDB companion
/// files (`.wal`, `.wal.lck`) if they exist, then verify that a fresh empty
/// database can be opened at the same path.
pub fn recover_db(db_path: &str, action: RecoveryAction) -> Result<RecoveryResult, String> {
    let path = Path::new(db_path);
    let wal_str = format!("{db_path}.wal");
    let wal_lck_str = format!("{db_path}.wal.lck");
    let wal = Path::new(&wal_str);
    let wal_lck = Path::new(&wal_lck_str);

    match action {
        RecoveryAction::BackupAndRecreate => {
            let timestamp = {
                // Use SystemTime for a simple YYYYMMDD_HHMMSS suffix without
                // pulling in the `chrono` crate directly here.
                use std::time::{SystemTime, UNIX_EPOCH};
                let secs = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                // Convert epoch seconds to a rough date/time string.
                // (Good enough for a backup filename; not timezone-aware.)
                let s = secs % (24 * 3600);
                let hh = s / 3600;
                let mm = (s % 3600) / 60;
                let ss = s % 60;
                // Days since epoch → approximate YYYYMMDD
                let days = secs / (24 * 3600);
                let year = 1970 + days / 365;
                let day_of_year = days % 365;
                let month = day_of_year / 30 + 1;
                let day = day_of_year % 30 + 1;
                format!("{year:04}{month:02}{day:02}_{hh:02}{mm:02}{ss:02}")
            };

            let backup_path = format!("{db_path}.corrupt.{timestamp}");

            // Rename main file
            if path.exists() {
                std::fs::rename(path, &backup_path)
                    .map_err(|e| format!("cannot rename database file: {e}"))?;
                info!(
                    db_path,
                    backup = %backup_path,
                    "db_health: renamed corrupt DB to backup"
                );
            }

            // Remove companion files (they belong to the old corrupted file)
            remove_if_exists(wal)?;
            remove_if_exists(wal_lck)?;

            // Verify a fresh database opens correctly
            verify_fresh_db(db_path)?;

            Ok(RecoveryResult {
                success: true,
                message: format!(
                    "Corrupt database backed up to {} and replaced with a fresh database.",
                    backup_path
                ),
                backup_path: Some(backup_path),
            })
        }

        RecoveryAction::DeleteAndRecreate => {
            remove_if_exists(path)?;
            remove_if_exists(wal)?;
            remove_if_exists(wal_lck)?;

            info!(db_path, "db_health: deleted corrupt DB files");

            verify_fresh_db(db_path)?;

            Ok(RecoveryResult {
                success: true,
                message: "Corrupt database deleted and replaced with a fresh database."
                    .to_string(),
                backup_path: None,
            })
        }
    }
}

// ---- helpers ----

fn remove_if_exists(path: &Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|e| format!("cannot remove {}: {e}", path.display()))?;
        info!(path = %path.display(), "db_health: removed file");
    }
    Ok(())
}

/// Open a brand-new DuckDB at `db_path` and run `SELECT 1` to prove it works.
fn verify_fresh_db(db_path: &str) -> Result<(), String> {
    let conn = duckdb::Connection::open(db_path)
        .map_err(|e| format!("cannot open fresh database: {e}"))?;
    conn.execute_batch("SELECT 1")
        .map_err(|e| format!("fresh database failed sanity check: {e}"))?;
    info!(db_path, "db_health: fresh database verified");
    Ok(())
}
