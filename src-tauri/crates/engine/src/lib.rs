mod analysis;
mod db_manager;
pub mod domain_pack;
mod executor;
mod export;
mod identifiers;
mod ingest;
mod overture;
mod schema;
mod types;

// Re-export geocode crate's public API for backward compatibility
pub use spatia_geocode::{
    cache_lookup, cache_store, ensure_cache_table,
    geocode_addresses, geocode_batch, geocode_batch_with_components, geocode_via_geocodio,
    AddressComponents, components_from_columns, components_from_string, extract_zip,
    GeocodeBatchResult, GeocodeResult, GeocodeStats,
};
pub use spatia_geocode::search_index;

pub use analysis::execute_analysis_sql_to_geojson;
pub use analysis::AnalysisExecutionResult;
pub use analysis::TabularResult;
pub use db_manager::DbManager;
pub use executor::execute_command;
pub use ingest::ingest_csv;
pub use ingest::ingest_csv_to_table;
pub use overture::overture_extract_to_table;
pub use overture::overture_geocode;
pub use overture::overture_search;
pub use overture::BBox;
pub use overture::OVERTURE_RELEASE;
pub use schema::fetch_column_samples;
pub use schema::raw_staging_schema;
pub use schema::table_schema;
pub use schema::TableColumn;
pub use domain_pack::{
    detect_domain_columns, format_domain_column_annotations, ColumnDetectionRule, DomainPack,
    UiConfig,
};
pub use export::{export_analysis_geojson, export_table_csv};
pub use identifiers::validate_table_name;
pub use types::EngineResult;
