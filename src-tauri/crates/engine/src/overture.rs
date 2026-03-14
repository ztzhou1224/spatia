use duckdb::Connection;
use serde::Serialize;

use crate::identifiers::validate_table_name;
use crate::EngineResult;

pub const OVERTURE_RELEASE: &str = "2026-02-18.0";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub xmin: f64,
    pub ymin: f64,
    pub xmax: f64,
    pub ymax: f64,
}

impl BBox {
    pub fn parse(input: &str) -> EngineResult<Self> {
        let parts: Vec<&str> = input.split(',').map(str::trim).collect();
        if parts.len() != 4 {
            return Err("bbox must be: xmin,ymin,xmax,ymax".into());
        }
        let xmin = parts[0].parse::<f64>()?;
        let ymin = parts[1].parse::<f64>()?;
        let xmax = parts[2].parse::<f64>()?;
        let ymax = parts[3].parse::<f64>()?;
        if !(xmin < xmax && ymin < ymax) {
            return Err("bbox must satisfy xmin < xmax and ymin < ymax".into());
        }
        Ok(Self {
            xmin,
            ymin,
            xmax,
            ymax,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OvertureExtractResult {
    pub status: &'static str,
    pub table: String,
    pub release: String,
    pub row_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OvertureSearchResult {
    pub id: Option<String>,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OvertureGeocodeResult {
    pub id: Option<String>,
    pub label: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

pub fn overture_extract_to_table(
    db_path: &str,
    theme: &str,
    item_type: &str,
    bbox: BBox,
    table_name: Option<&str>,
) -> EngineResult<OvertureExtractResult> {
    let table = table_name
        .map(str::to_string)
        .unwrap_or_else(|| default_table_name(theme, item_type));
    validate_table_name(&table)?;

    let conn = Connection::open(db_path)?;
    ensure_extensions(&conn)?;

    let release = overture_release();
    let source_path = overture_source_path(&release, theme, item_type);
    let sql = format!(
        "CREATE OR REPLACE TABLE {table} AS \
         SELECT * FROM read_parquet('{source}') \
         WHERE bbox.xmin <= {xmax} AND bbox.xmax >= {xmin} \
           AND bbox.ymin <= {ymax} AND bbox.ymax >= {ymin}",
        table = table,
        source = source_path,
        xmin = bbox.xmin,
        ymin = bbox.ymin,
        xmax = bbox.xmax,
        ymax = bbox.ymax,
    );
    conn.execute(&sql, [])?;
    create_lookup_table(&conn, &table, theme)?;

    // Build Tantivy search index for the lookup table
    let lookup = lookup_table_name(&table);
    let index_dir = spatia_geocode::search_index::index_dir_for_table(db_path, &lookup);
    match spatia_geocode::search_index::build_index(&conn, &lookup, &index_dir) {
        Ok(count) => {
            tracing::info!(
                doc_count = count,
                lookup_table = lookup.as_str(),
                "overture_extract: built Tantivy search index"
            );
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                lookup_table = lookup.as_str(),
                "overture_extract: failed to build Tantivy index, LIKE fallback will be used"
            );
        }
    }

    let count_sql = format!("SELECT COUNT(*) FROM {table}", table = table);
    let mut stmt = conn.prepare(&count_sql)?;
    let row_count: i64 = stmt.query_row([], |row| row.get(0))?;

    Ok(OvertureExtractResult {
        status: "ok",
        table,
        release,
        row_count,
    })
}

pub fn overture_search(
    db_path: &str,
    table_name: &str,
    query: &str,
    limit: usize,
) -> EngineResult<Vec<OvertureSearchResult>> {
    validate_table_name(table_name)?;
    if query.trim().is_empty() {
        return Err("search query cannot be empty".into());
    }
    let safe_limit = limit.clamp(1, 1000);

    let conn = Connection::open(db_path)?;
        let lookup_table = lookup_table_name(table_name);
        validate_table_name(&lookup_table)?;

        let escaped_query = query.replace('\'', "''").to_lowercase();
    let sql = format!(
                "SELECT source_id AS id, label \
                 FROM {table} \
                 WHERE label_norm LIKE '%{query}%' \
                 ORDER BY \
                     CASE \
                         WHEN label_norm = '{query}' THEN 0 \
                         WHEN label_norm LIKE '{query}%' THEN 1 \
                         WHEN label_norm LIKE '% {query}%' THEN 2 \
                         ELSE 3 \
                     END, \
                     length(label_norm), \
                     label \
         LIMIT {limit}",
                table = lookup_table,
        query = escaped_query,
        limit = safe_limit,
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(OvertureSearchResult {
            id: row.get(0).ok(),
            label: row.get::<_, String>(1).unwrap_or_default(),
        });
    }
    Ok(out)
}

pub fn overture_geocode(
    db_path: &str,
    table_name: &str,
    query: &str,
    limit: usize,
) -> EngineResult<Vec<OvertureGeocodeResult>> {
    validate_table_name(table_name)?;
    if query.trim().is_empty() {
        return Err("geocode query cannot be empty".into());
    }
    let safe_limit = limit.clamp(1, 1000);

    let conn = Connection::open(db_path)?;
    ensure_extensions(&conn)?;

    let lookup_table = lookup_table_name(table_name);
    validate_table_name(&lookup_table)?;
    let escaped_query = query.replace('\'', "''").to_lowercase();

    let sql = format!(
        "SELECT \
           l.source_id AS id, \
           l.label, \
           CAST(ST_Y(t.geometry) AS DOUBLE) AS lat, \
           CAST(ST_X(t.geometry) AS DOUBLE) AS lon \
         FROM {lookup} l \
         JOIN {table} t ON CAST(t.id AS VARCHAR) = l.source_id \
         WHERE l.label_norm LIKE '%{query}%' \
         ORDER BY \
           CASE \
             WHEN l.label_norm = '{query}' THEN 0 \
             WHEN l.label_norm LIKE '{query}%' THEN 1 \
             WHEN l.label_norm LIKE '% {query}%' THEN 2 \
             ELSE 3 \
           END, \
           length(l.label_norm), \
           l.label \
         LIMIT {limit}",
        lookup = lookup_table,
        table = table_name,
        query = escaped_query,
        limit = safe_limit,
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(OvertureGeocodeResult {
            id: row.get(0).ok(),
            label: row.get::<_, String>(1).unwrap_or_default(),
            lat: row.get(2).ok(),
            lon: row.get(3).ok(),
        });
    }
    Ok(out)
}

fn create_lookup_table(conn: &Connection, table_name: &str, theme: &str) -> EngineResult<()> {
        let lookup_table = lookup_table_name(table_name);
        validate_table_name(&lookup_table)?;

        let sql = if theme == "addresses" {
                format!(
                        "CREATE OR REPLACE TABLE {lookup} AS \
                         SELECT \
                             CAST(id AS VARCHAR) AS source_id, \
                             trim(regexp_replace( \
                                 concat_ws(' ', \
                                     coalesce(number, ''), \
                                     coalesce(street, ''), \
                                     coalesce(postal_city, ''), \
                                     coalesce(postcode, ''), \
                                     coalesce(country, '') \
                                 ), \
                                 '\\s+', \
                                 ' ' \
                             )) AS label, \
                             lower(trim(regexp_replace( \
                                 concat_ws(' ', \
                                     coalesce(number, ''), \
                                     coalesce(street, ''), \
                                     coalesce(postal_city, ''), \
                                     coalesce(postcode, ''), \
                                     coalesce(country, '') \
                                 ), \
                                 '\\s+', \
                                 ' ' \
                             ))) AS label_norm \
                         FROM {source} \
                         WHERE trim(regexp_replace( \
                                 concat_ws(' ', \
                                     coalesce(number, ''), \
                                     coalesce(street, ''), \
                                     coalesce(postal_city, ''), \
                                     coalesce(postcode, ''), \
                                     coalesce(country, '') \
                                 ), \
                                 '\\s+', \
                                 ' ' \
                             )) != ''",
                        lookup = lookup_table,
                        source = table_name
                )
        } else if has_column(conn, table_name, "names")? {
                format!(
                        "CREATE OR REPLACE TABLE {lookup} AS \
                         SELECT \
                             CAST(id AS VARCHAR) AS source_id, \
                             trim(CAST(names AS VARCHAR)) AS label, \
                             lower(trim(CAST(names AS VARCHAR))) AS label_norm \
                         FROM {source} \
                         WHERE names IS NOT NULL \
                             AND trim(CAST(names AS VARCHAR)) != ''",
                        lookup = lookup_table,
                        source = table_name
                )
        } else {
                format!(
                        "CREATE OR REPLACE TABLE {lookup} AS \
                         SELECT \
                             CAST(id AS VARCHAR) AS source_id, \
                             CAST(id AS VARCHAR) AS label, \
                             lower(CAST(id AS VARCHAR)) AS label_norm \
                         FROM {source}",
                        lookup = lookup_table,
                        source = table_name
                )
        };

        conn.execute(&sql, [])?;
        Ok(())
}

fn has_column(conn: &Connection, table_name: &str, column: &str) -> EngineResult<bool> {
        let mut stmt = conn.prepare(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = 'main' AND table_name = ? \
             ORDER BY ordinal_position"
        )?;
        let mut rows = stmt.query(duckdb::params![table_name])?;

        while let Some(row) = rows.next()? {
                let name: String = row.get(0)?;
                if name.eq_ignore_ascii_case(column) {
                        return Ok(true);
                }
        }
        Ok(false)
}

fn ensure_extensions(conn: &Connection) -> EngineResult<()> {
    conn.execute("INSTALL spatial", [])?;
    conn.execute("LOAD spatial", [])?;
    conn.execute("INSTALL httpfs", [])?;
    conn.execute("LOAD httpfs", [])?;
    Ok(())
}

fn overture_source_path(release: &str, theme: &str, item_type: &str) -> String {
    if theme == "places" {
        return format!(
            "s3://overturemaps-us-west-2/release/{}/theme=places/*/*",
            release
        );
    }

    if item_type.trim().is_empty() || item_type == "*" {
        return format!(
            "s3://overturemaps-us-west-2/release/{}/theme={}/*",
            release, theme
        );
    }

    format!(
        "s3://overturemaps-us-west-2/release/{}/theme={}/type={}/*",
        release, theme, item_type
    )
}

fn overture_release() -> String {
    std::env::var("SPATIA_OVERTURE_RELEASE").unwrap_or_else(|_| OVERTURE_RELEASE.to_string())
}

fn default_table_name(theme: &str, item_type: &str) -> String {
    let normalized_theme = theme.replace('-', "_");
    let normalized_type = item_type.replace('-', "_");
    format!("overture_{normalized_theme}_{normalized_type}")
}

fn lookup_table_name(base_table: &str) -> String {
    format!("{base_table}_lookup")
}

/// Download Overture building footprints within a bounding box and cache in DuckDB.
/// Returns a GeoJSON FeatureCollection as a String.
pub fn fetch_buildings_in_bbox(
    db_path: &str,
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
) -> EngineResult<String> {
    let conn = Connection::open(db_path)?;
    ensure_extensions(&conn)?;

    // Create cache table if it doesn't exist
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS overture_buildings_cache (
            gers_id VARCHAR PRIMARY KEY,
            height DOUBLE,
            num_floors INTEGER,
            geometry VARCHAR
        )",
    )?;

    // Check if buildings in this bbox are already cached
    let cached_count: i64 = {
        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM overture_buildings_cache \
             WHERE geometry IS NOT NULL \
             AND ST_Intersects(ST_GeomFromText(geometry), ST_MakeEnvelope(?, ?, ?, ?))",
        )?;
        stmt.query_row(
            duckdb::params![xmin, ymin, xmax, ymax],
            |row| row.get(0),
        )
        .unwrap_or(0)
    };

    if cached_count == 0 {
        // Fetch from Overture S3
        let release = overture_release();
        let source_path = format!(
            "s3://overturemaps-us-west-2/release/{}/theme=buildings/type=building/*",
            release
        );
        let insert_sql = format!(
            "INSERT OR IGNORE INTO overture_buildings_cache \
             SELECT \
               id AS gers_id, \
               CAST(height AS DOUBLE) AS height, \
               CAST(num_floors AS INTEGER) AS num_floors, \
               ST_AsText(geometry) AS geometry \
             FROM read_parquet('{source}', hive_partitioning=true) \
             WHERE bbox.xmin >= {xmin} AND bbox.xmax <= {xmax} \
               AND bbox.ymin >= {ymin} AND bbox.ymax <= {ymax}",
            source = source_path,
            xmin = xmin,
            xmax = xmax,
            ymin = ymin,
            ymax = ymax,
        );
        conn.execute_batch(&insert_sql)?;
    }

    // Query cached buildings within bbox and convert to GeoJSON
    let mut stmt = conn.prepare(
        "SELECT gers_id, height, num_floors, geometry \
         FROM overture_buildings_cache \
         WHERE geometry IS NOT NULL \
           AND ST_Intersects(ST_GeomFromText(geometry), ST_MakeEnvelope(?, ?, ?, ?))",
    )?;

    let mut features: Vec<serde_json::Value> = Vec::new();
    let mut rows = stmt.query(duckdb::params![xmin, ymin, xmax, ymax])?;

    while let Some(row) = rows.next()? {
        let gers_id: Option<String> = row.get(0).ok();
        let height: Option<f64> = row.get(1).ok();
        let num_floors: Option<i32> = row.get(2).ok();
        let wkt: String = row.get(3)?;

        // Convert WKT to GeoJSON geometry via DuckDB ST_AsGeoJSON
        let geom_json: Option<serde_json::Value> = {
            let mut geom_stmt = conn.prepare(
                "SELECT ST_AsGeoJSON(ST_GeomFromText(?))",
            )?;
            geom_stmt
                .query_row(duckdb::params![wkt], |r| r.get::<_, String>(0))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
        };

        if let Some(geometry) = geom_json {
            features.push(serde_json::json!({
                "type": "Feature",
                "geometry": geometry,
                "properties": {
                    "gers_id": gers_id,
                    "height": height,
                    "num_floors": num_floors,
                }
            }));
        }
    }

    let fc = serde_json::json!({
        "type": "FeatureCollection",
        "features": features,
    });
    serde_json::to_string(&fc).map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use super::{
        default_table_name, lookup_table_name, overture_source_path, BBox, OVERTURE_RELEASE,
    };

    #[test]
    fn bbox_parse_success() {
        let bbox = BBox::parse("-122.4,47.5,-122.2,47.7").expect("parse bbox");
        assert_eq!(bbox.xmin, -122.4);
        assert_eq!(bbox.ymin, 47.5);
        assert_eq!(bbox.xmax, -122.2);
        assert_eq!(bbox.ymax, 47.7);
    }

    #[test]
    fn bbox_parse_rejects_invalid_order() {
        let err = BBox::parse("1,1,0,2").expect_err("should fail");
        assert!(err.to_string().contains("xmin < xmax"));
    }

    #[test]
    fn source_path_uses_pinned_release() {
        let path = overture_source_path(OVERTURE_RELEASE, "places", "place");
        assert!(path.contains(OVERTURE_RELEASE));
        assert!(path.contains("theme=places"));
        assert!(!path.contains("type=place"));
    }

    #[test]
    fn source_path_uses_type_partition_for_transportation() {
        let path = overture_source_path(OVERTURE_RELEASE, "transportation", "segment");
        assert!(path.contains("theme=transportation"));
        assert!(path.contains("type=segment"));
    }

    #[test]
    fn default_table_name_normalizes_dashes() {
        assert_eq!(default_table_name("base", "land-use"), "overture_base_land_use");
    }

    #[test]
    fn lookup_table_suffix() {
        assert_eq!(lookup_table_name("overture_places_place"), "overture_places_place_lookup");
    }

    #[test]
    fn source_path_addresses_type_partition() {
        let path = overture_source_path(OVERTURE_RELEASE, "addresses", "address");
        assert!(path.contains("theme=addresses"));
        assert!(path.contains("type=address"));
    }
}
