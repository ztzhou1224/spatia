mod cache;
mod geocode;
mod geocodio;
mod identifiers;
pub mod overture_cache;
mod scoring;
mod text;
mod types;
pub mod search_index;

pub use cache::{cache_lookup, cache_store, ensure_cache_table};
pub use geocode::{geocode_addresses, geocode_batch, geocode_batch_with_components, local_fuzzy_geocode};
pub use geocodio::geocode_via_geocodio;
pub use scoring::{score_candidate, MIN_LOCAL_ACCEPT_SCORE, MIN_SCORE};
pub use text::{
    components_from_columns, components_from_string, extract_zip, normalize_address,
    tokenize_address, AddressComponents,
};
pub use types::{GeoResult, GeocodeBatchResult, GeocodeResult, GeocodeStats};
