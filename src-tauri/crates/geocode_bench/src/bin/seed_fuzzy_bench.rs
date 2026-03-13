//! One-time seed script: extract ~500 real Overture address labels from S3
//! into `data/fuzzy_bench_addresses.csv`.
//!
//! Requires internet access (downloads from S3 via httpfs).
//!
//! Usage:
//!   cargo run -p spatia_geocode_bench --bin seed_fuzzy_bench

use std::path::PathBuf;

use duckdb::Connection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let output_path = project_data_dir().join("fuzzy_bench_addresses.csv");
    if output_path.exists() {
        println!(
            "Output file already exists: {}",
            output_path.display()
        );
        println!("Delete it first if you want to regenerate.");
        return Ok(());
    }

    println!("Creating temp DuckDB and loading extensions...");
    let db_path = "/tmp/spatia_seed_fuzzy_bench.duckdb";
    let conn = Connection::open(db_path)?;
    conn.execute_batch("INSTALL spatial; LOAD spatial")?;
    conn.execute_batch("INSTALL httpfs; LOAD httpfs")?;

    let overture_release = std::env::var("SPATIA_OVERTURE_RELEASE")
        .unwrap_or_else(|_| "2025-01-22.0".to_string());

    // Seattle bounding box
    let bbox = "-122.4,47.5,-122.2,47.7";
    let parts: Vec<f64> = bbox.split(',').map(|s| s.parse().unwrap()).collect();
    let (xmin, ymin, xmax, ymax) = (parts[0], parts[1], parts[2], parts[3]);

    println!(
        "Extracting Overture addresses for bbox={} (release={})...",
        bbox, overture_release
    );

    let sql = format!(
        r#"
        SELECT
            id,
            concat_ws(' ',
                JSON_EXTRACT_STRING(addresses, '$[0].number'),
                JSON_EXTRACT_STRING(addresses, '$[0].street'),
                JSON_EXTRACT_STRING(addresses, '$[0].postal_city'),
                JSON_EXTRACT_STRING(addresses, '$[0].postcode'),
                JSON_EXTRACT_STRING(addresses, '$[0].country')
            ) AS label,
            ST_Y(ST_GeomFromWKB(geometry)) AS lat,
            ST_X(ST_GeomFromWKB(geometry)) AS lon
        FROM read_parquet(
            's3://overturemaps-us-west-2/release/{release}/theme=addresses/type=address/*',
            hive_partitioning=true
        )
        WHERE bbox.xmin >= {xmin}
          AND bbox.xmax <= {xmax}
          AND bbox.ymin >= {ymin}
          AND bbox.ymax <= {ymax}
        ORDER BY random()
        LIMIT 500
        "#,
        release = overture_release,
        xmin = xmin,
        ymin = ymin,
        xmax = xmax,
        ymax = ymax,
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    // Ensure data directory exists
    std::fs::create_dir_all(output_path.parent().unwrap())?;

    let mut wtr = csv::Writer::from_path(&output_path)?;
    wtr.write_record(["id", "label", "lat", "lon"])?;

    let mut count = 0;
    while let Some(row) = rows.next()? {
        let id: String = row.get::<_, String>(0).unwrap_or_default();
        let label: String = row.get::<_, String>(1).unwrap_or_default();
        let lat: f64 = row.get::<_, f64>(2).unwrap_or(0.0);
        let lon: f64 = row.get::<_, f64>(3).unwrap_or(0.0);

        // Skip entries with empty labels
        if label.trim().is_empty() || label.trim() == "US" {
            continue;
        }

        wtr.write_record([&id, &label, &lat.to_string(), &lon.to_string()])?;
        count += 1;
    }

    wtr.flush()?;

    // Cleanup temp DB
    drop(conn);
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(format!("{db_path}.wal"));
    let _ = std::fs::remove_file(format!("{db_path}.wal.lck"));

    println!(
        "Wrote {} addresses to {}",
        count,
        output_path.display()
    );

    Ok(())
}

fn project_data_dir() -> PathBuf {
    // Navigate from src-tauri/crates/geocode_bench/ up to repo root, then data/
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent() // crates/
        .and_then(|p| p.parent()) // src-tauri/
        .and_then(|p| p.parent()) // repo root
        .map(|p| p.join("data"))
        .unwrap_or_else(|| PathBuf::from("data"))
}
