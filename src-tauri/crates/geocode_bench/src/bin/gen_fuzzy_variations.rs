//! One-time LLM-powered variation generator: reads ground truth addresses
//! and generates realistic user-typed variations via Gemini.
//!
//! Requires `SPATIA_GEMINI_API_KEY` environment variable.
//!
//! Usage:
//!   cargo run -p spatia_geocode_bench --bin gen_fuzzy_variations

use std::path::PathBuf;

use serde::Deserialize;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let data_dir = project_data_dir();
    let gt_path = data_dir.join("fuzzy_bench_addresses.csv");
    let out_path = data_dir.join("fuzzy_bench_variations.csv");

    if !gt_path.exists() {
        eprintln!(
            "ERROR: ground truth CSV not found at {}",
            gt_path.display()
        );
        eprintln!("Run `cargo run -p spatia_geocode_bench --bin seed_fuzzy_bench` first.");
        std::process::exit(1);
    }

    if out_path.exists() {
        println!(
            "Output file already exists: {}",
            out_path.display()
        );
        println!("Delete it first if you want to regenerate.");
        return Ok(());
    }

    let api_key = std::env::var("SPATIA_GEMINI_API_KEY")
        .map_err(|_| "SPATIA_GEMINI_API_KEY environment variable is not set")?;

    // Read ground truth
    let mut rdr = csv::Reader::from_path(&gt_path)?;
    let mut ground_truth: Vec<GtRow> = Vec::new();
    for result in rdr.deserialize() {
        let row: GtRow = result?;
        ground_truth.push(row);
    }
    println!("Loaded {} ground truth addresses", ground_truth.len());

    // Process in batches of 25
    let batch_size = 25;
    let mut all_variations: Vec<VariationOutput> = Vec::new();
    let total_batches = ground_truth.len().div_ceil(batch_size);

    let rt = tokio::runtime::Runtime::new()?;

    for (batch_idx, chunk) in ground_truth.chunks(batch_size).enumerate() {
        print!(
            "  Batch [{}/{}] ({} addresses)... ",
            batch_idx + 1,
            total_batches,
            chunk.len()
        );
        use std::io::Write;
        let _ = std::io::stdout().flush();

        let prompt = build_variation_prompt(chunk);
        let result = rt.block_on(call_gemini(&api_key, &prompt));

        match result {
            Ok(variations) => {
                println!("{} variations", variations.len());
                all_variations.extend(variations);
            }
            Err(e) => {
                println!("ERROR: {}", e);
                eprintln!(
                    "  Failed on batch {} — continuing with remaining batches",
                    batch_idx + 1
                );
            }
        }
    }

    // Write output CSV
    let mut wtr = csv::Writer::from_path(&out_path)?;
    wtr.write_record(["original_id", "user_input", "variation_type"])?;
    for v in &all_variations {
        wtr.write_record([&v.original_id, &v.user_input, &v.variation_type])?;
    }
    wtr.flush()?;

    println!(
        "\nWrote {} variations to {}",
        all_variations.len(),
        out_path.display()
    );

    Ok(())
}

#[derive(Debug, Deserialize)]
struct GtRow {
    id: String,
    label: String,
    #[allow(dead_code)]
    lat: f64,
    #[allow(dead_code)]
    lon: f64,
}

#[derive(Debug, Deserialize)]
struct VariationOutput {
    original_id: String,
    user_input: String,
    variation_type: String,
}

fn build_variation_prompt(addresses: &[GtRow]) -> String {
    let mut addr_list = String::new();
    for addr in addresses {
        addr_list.push_str(&format!("- id=\"{}\", label=\"{}\"\n", addr.id, addr.label));
    }

    format!(
        r#"You are generating test data for a geocoding benchmark. For each address below,
generate 1-3 realistic variations that a real user might type when searching for this address.

Variation types to include (use these exact labels):
- "abbreviation": Use common abbreviations (Street→St, Avenue→Ave, Boulevard→Blvd, Drive→Dr, etc.)
- "dropped_zip": Drop the ZIP/postal code
- "dropped_city": Drop the city name
- "informal": Casual/informal version (e.g., "123 Main" instead of "123 Main Street Springfield 62704 US")
- "typo": Include a minor typo (one character off)
- "reordered": Reorder components (e.g., city before street)
- "mixed": Combination of multiple variations

Each address should get 1-3 variations. Distribute the variation types across the full set.

Addresses:
{addr_list}

Return a JSON array where each element has:
- "original_id": the id of the source address
- "user_input": the variation text
- "variation_type": one of the labels above

Return ONLY the JSON array, no other text."#,
        addr_list = addr_list
    )
}

async fn call_gemini(
    api_key: &str,
    prompt: &str,
) -> Result<Vec<VariationOutput>, Box<dyn std::error::Error>> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}",
        api_key
    );

    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "contents": [{
            "parts": [{"text": prompt}]
        }],
        "generation_config": {
            "response_mime_type": "application/json",
            "temperature": 0.7
        }
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    let parsed: serde_json::Value = resp.json().await?;

    let text = parsed["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or("no text in Gemini response")?;

    let variations: Vec<VariationOutput> = serde_json::from_str(text)?;
    Ok(variations)
}

fn project_data_dir() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.join("data"))
        .unwrap_or_else(|| PathBuf::from("data"))
}
