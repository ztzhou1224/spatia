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
    /// Overture GERS ID for linking to building footprints and 3D rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gers_id: Option<String>,
}

/// Source breakdown stats returned alongside geocoding results.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GeocodeStats {
    pub total: usize,
    pub geocoded: usize,
    pub cache_hits: usize,
    pub overture_exact: usize,
    pub local_fuzzy: usize,
    pub api_resolved: usize,
    pub unresolved: usize,
}

/// Progress update emitted during geocoding (especially the Nominatim phase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeocodeProgressUpdate {
    /// Pipeline stage: "cache", "overture", "nominatim", "done"
    pub stage: String,
    /// Number of addresses processed so far in the current stage.
    pub processed: usize,
    /// Total addresses to process in the current stage.
    pub total: usize,
    /// Estimated seconds remaining (meaningful during Nominatim phase).
    pub estimated_secs: Option<u64>,
    /// The address currently being processed.
    pub current_address: Option<String>,
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
