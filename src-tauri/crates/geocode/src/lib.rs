mod cache;
mod geocode;
mod geocodio;
mod identifiers;
mod scoring;
mod text;
mod types;
pub mod search_index;

pub use cache::{cache_lookup, cache_store, ensure_cache_table};
pub use geocode::{geocode_addresses, geocode_batch};
pub use geocodio::geocode_via_geocodio;
pub use scoring::{score_candidate, MIN_LOCAL_ACCEPT_SCORE, MIN_SCORE};
pub use text::{normalize_address, tokenize_address};
pub use types::{GeoResult, GeocodeBatchResult, GeocodeResult, GeocodeStats};
