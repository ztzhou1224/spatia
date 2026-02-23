use serde::{Deserialize, Serialize};

use crate::EngineResult;

pub const DEFAULT_GEOCODER_URL: &str = "http://127.0.0.1:7788";

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
