use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct TestCase {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,

    /// CSV file to ingest for test setup
    pub setup_csv: Option<String>,
    /// Table name for ingested CSV
    pub setup_table: Option<String>,

    /// Addresses to geocode
    pub addresses: Vec<String>,

    /// Pre-seed cache with known results before test
    #[serde(default)]
    pub seed_cache: bool,
    /// Cache seed entries: list of {address, lat, lon, source}
    #[serde(default)]
    pub cache_seeds: Vec<CacheSeed>,

    /// Set up a lookup table for local fuzzy matching
    #[serde(default)]
    pub setup_lookup: Option<LookupSetup>,

    /// Expected results (one per address, matched by address field)
    #[serde(default)]
    pub expect: Vec<ExpectedResult>,

    /// Expected total geocoded count
    pub expect_geocoded_count: Option<usize>,
    /// Expected unresolved count
    pub expect_unresolved_count: Option<usize>,

    /// Per-test timeout override
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheSeed {
    pub address: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "geocodio".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct LookupSetup {
    /// Name of the base table (e.g., "places")
    pub base_table: String,
    /// Entries for the base table: {id, label, lat, lon}
    pub entries: Vec<LookupEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LookupEntry {
    pub id: String,
    pub label: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExpectedResult {
    pub address: String,
    /// Max distance in meters from ground truth (default 500)
    #[serde(default = "default_max_distance")]
    pub max_distance_meters: f64,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Expected source: "cache", "overture_fuzzy", "geocodio"
    pub expect_source: Option<String>,
    /// Minimum confidence score
    pub min_confidence: Option<f64>,
}

fn default_max_distance() -> f64 {
    500.0
}

#[derive(Debug, Deserialize)]
pub struct Corpus {
    pub tests: Vec<TestCase>,
}

impl Corpus {
    pub fn from_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    pub fn filter_by_tags(&self, tags: &[String]) -> Vec<&TestCase> {
        if tags.is_empty() {
            return self.tests.iter().collect();
        }
        self.tests
            .iter()
            .filter(|tc| tc.tags.iter().any(|t| tags.contains(t)))
            .collect()
    }
}
