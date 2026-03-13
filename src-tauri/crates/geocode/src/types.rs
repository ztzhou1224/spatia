use serde::{Deserialize, Serialize};

/// Crate-level result type.
pub type GeoResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A geocoded address result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeocodeResult {
    pub address: String,
    pub lat: f64,
    pub lon: f64,
    pub source: String,
}

/// A richer geocoding result used by the batch-first smart geocoder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeocodeBatchResult {
    pub address: String,
    pub lat: f64,
    pub lon: f64,
    pub source: String,
    pub confidence: f64,
    pub matched_label: Option<String>,
    pub matched_table: Option<String>,
}

/// Source breakdown stats returned alongside geocoding results.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GeocodeStats {
    pub total: usize,
    pub geocoded: usize,
    pub cache_hits: usize,
    pub local_fuzzy: usize,
    pub api_resolved: usize,
    pub unresolved: usize,
}

impl From<GeocodeBatchResult> for GeocodeResult {
    fn from(value: GeocodeBatchResult) -> Self {
        Self {
            address: value.address,
            lat: value.lat,
            lon: value.lon,
            source: value.source,
        }
    }
}
