use std::path::Path;

use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, STORED, STRING,
};
use tantivy::{doc, Index, IndexWriter, Term};
use tracing::{debug, info};

use crate::geocode::tokenize_address;
use crate::EngineResult;

/// Pre-process an address string for Tantivy indexing/querying:
/// normalize, expand abbreviations, remove noise tokens.
pub fn preprocess_address(address: &str) -> String {
    let tokens = tokenize_address(address);
    let processed: Vec<String> = tokens
        .iter()
        .map(|t| expand_abbreviation(t).to_string())
        .filter(|t| !is_noise_token(t))
        .collect();
    processed.join(" ")
}

/// Expand common street-type, directional, and unit abbreviations.
fn expand_abbreviation(token: &str) -> &str {
    match token {
        "st" => "street",
        "ave" => "avenue",
        "blvd" => "boulevard",
        "dr" => "drive",
        "ln" => "lane",
        "rd" => "road",
        "ct" => "court",
        "cir" => "circle",
        "pl" => "place",
        "ter" => "terrace",
        "hwy" => "highway",
        "pkwy" => "parkway",
        "sq" => "square",
        "n" => "north",
        "s" => "south",
        "e" => "east",
        "w" => "west",
        "ne" => "northeast",
        "nw" => "northwest",
        "se" => "southeast",
        "sw" => "southwest",
        "apt" => "apartment",
        "ste" => "suite",
        "fl" => "floor",
        other => other,
    }
}

const US_STATE_ABBREVS: &[&str] = &[
    "al", "ak", "az", "ar", "ca", "co", "ct", "de", "fl", "ga", "hi", "id", "il", "in", "ia",
    "ks", "ky", "la", "me", "md", "ma", "mi", "mn", "ms", "mo", "mt", "ne", "nv", "nh", "nj",
    "nm", "ny", "nc", "nd", "oh", "ok", "or", "pa", "ri", "sc", "sd", "tn", "tx", "ut", "vt",
    "va", "wa", "wv", "wi", "wy", "dc",
];

fn is_noise_token(token: &str) -> bool {
    US_STATE_ABBREVS.contains(&token) || matches!(token, "us" | "usa")
}

// ---- Index Schema ----

struct IndexFields {
    source_id: Field,
    label: Field,
    label_norm: Field,
}

fn build_schema() -> (Schema, IndexFields) {
    let mut builder = Schema::builder();

    let source_id = builder.add_text_field("source_id", STRING | STORED);
    let label = builder.add_text_field("label", STORED);

    // label_norm: indexed for full-text search, not stored (we use label for display)
    let text_opts = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let label_norm = builder.add_text_field("label_norm", text_opts);

    let schema = builder.build();
    (
        schema,
        IndexFields {
            source_id,
            label,
            label_norm,
        },
    )
}

// ---- Index Building ----

/// Build a Tantivy index from a DuckDB lookup table.
///
/// The lookup table must have columns: `source_id`, `label`, `label_norm`.
/// The index is written to `index_dir`.
pub fn build_index(
    conn: &duckdb::Connection,
    lookup_table: &str,
    index_dir: &Path,
) -> EngineResult<usize> {
    crate::identifiers::validate_table_name(lookup_table)?;

    // Clean up any existing index
    if index_dir.exists() {
        std::fs::remove_dir_all(index_dir)?;
    }
    std::fs::create_dir_all(index_dir)?;

    let (schema, fields) = build_schema();
    let index = Index::create_in_dir(index_dir, schema)?;

    // 50MB heap for indexing
    let mut writer: IndexWriter = index.writer(50_000_000)?;

    let sql = format!(
        "SELECT source_id, label, label_norm FROM {lookup_table}",
        lookup_table = lookup_table
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;

    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        let source_id: String = row.get::<_, String>(0).unwrap_or_default();
        let label: String = row.get::<_, String>(1).unwrap_or_default();
        let label_norm: String = row.get::<_, String>(2).unwrap_or_default();

        // Pre-process label_norm for indexing (expand abbreviations, remove noise)
        let processed = preprocess_address(&label_norm);
        if processed.is_empty() {
            continue;
        }

        writer.add_document(doc!(
            fields.source_id => source_id,
            fields.label => label,
            fields.label_norm => processed,
        ))?;
        count += 1;
    }

    writer.commit()?;
    info!(
        doc_count = count,
        index_dir = %index_dir.display(),
        "search_index: built Tantivy index for {lookup_table}"
    );
    Ok(count)
}

// ---- Index Querying ----

/// Search result from a Tantivy index query.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub source_id: String,
    pub label: String,
    pub score: f64,
}

/// Search for addresses in a Tantivy index using BM25 + fuzzy matching.
///
/// Returns up to `top_k` results with scores normalized to [0, 1].
pub fn search_addresses(
    index_dir: &Path,
    query: &str,
    top_k: usize,
) -> EngineResult<Vec<SearchHit>> {
    if !index_dir.exists() {
        return Ok(Vec::new());
    }

    let (_schema, fields) = build_schema();
    let index = Index::open_in_dir(index_dir)?;
    let reader = index.reader()?;
    let searcher = reader.searcher();

    // Pre-process query the same way we pre-processed index content
    let processed_query = preprocess_address(query);
    let tokens: Vec<&str> = processed_query.split_whitespace().collect();
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    // Build a BooleanQuery with boosted terms
    let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    for token in &tokens {
        let is_numeric = token.chars().all(|c| c.is_ascii_digit());
        let is_postcode = is_numeric && token.len() >= 5;

        // Determine boost factor
        let boost = if is_postcode {
            3.0
        } else if is_numeric {
            2.0
        } else {
            1.0
        };

        // Exact term query (always)
        let term = Term::from_field_text(fields.label_norm, token);
        let exact_query = TermQuery::new(term.clone(), IndexRecordOption::WithFreqs);
        let boosted = BoostQuery::new(Box::new(exact_query), boost);
        subqueries.push((Occur::Should, Box::new(boosted)));

        // Fuzzy term query for non-numeric tokens > 4 chars
        if !is_numeric && token.len() > 4 {
            let fuzzy_query = FuzzyTermQuery::new(term, 1, true);
            let boosted_fuzzy = BoostQuery::new(Box::new(fuzzy_query), boost * 0.8);
            subqueries.push((Occur::Should, Box::new(boosted_fuzzy)));
        }
    }

    let bool_query = BooleanQuery::new(subqueries);
    let top_docs = searcher.search(&bool_query, &TopDocs::with_limit(top_k))?;

    if top_docs.is_empty() {
        return Ok(Vec::new());
    }

    // Normalize scores: divide by max score
    let max_score = top_docs
        .iter()
        .map(|(score, _)| *score)
        .fold(f32::NEG_INFINITY, f32::max);

    let mut results = Vec::new();
    for (raw_score, doc_address) in &top_docs {
        let doc = searcher.doc::<tantivy::TantivyDocument>(*doc_address)?;

        let source_id = doc
            .get_first(fields.source_id)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let label = doc
            .get_first(fields.label)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Normalize to [0, 1] with floor of 0.3
        let normalized = if max_score > 0.0 {
            (*raw_score as f64 / max_score as f64).max(0.3)
        } else {
            0.3
        };

        results.push(SearchHit {
            source_id,
            label,
            score: normalized,
        });
    }

    debug!(
        hits = results.len(),
        query = query,
        "search_index: search complete"
    );
    Ok(results)
}

/// Determine the index directory path for a given lookup table and DB path.
pub fn index_dir_for_table(db_path: &str, lookup_table: &str) -> std::path::PathBuf {
    let db_dir = Path::new(db_path)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    db_dir.join("indexes").join(lookup_table)
}

/// Check if a Tantivy index exists and is non-empty for the given lookup table.
pub fn has_index(db_path: &str, lookup_table: &str) -> bool {
    let dir = index_dir_for_table(db_path, lookup_table);
    if !dir.exists() {
        return false;
    }
    // Try to open the index to verify it's valid
    match Index::open_in_dir(&dir) {
        Ok(index) => match index.reader() {
            Ok(reader) => reader.searcher().num_docs() > 0,
            Err(_) => false,
        },
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use duckdb::Connection;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_suffix() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    }

    fn create_test_lookup_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE test_lookup (
                source_id VARCHAR,
                label VARCHAR,
                label_norm VARCHAR
            )",
        )
        .unwrap();

        let entries = vec![
            ("1", "123 Main Street Springfield 62704 US", "123 main street springfield 62704 us"),
            ("2", "456 Oak Avenue Chicago 60601 US", "456 oak avenue chicago 60601 us"),
            ("3", "789 Elm Boulevard Seattle 98101 US", "789 elm boulevard seattle 98101 us"),
            ("4", "101 Pine Drive Portland 97201 US", "101 pine drive portland 97201 us"),
            ("5", "202 Maple Lane Denver 80202 US", "202 maple lane denver 80202 us"),
            ("6", "303 Cedar Court Austin 78701 US", "303 cedar court austin 78701 us"),
            ("7", "404 Birch Road Boston 02101 US", "404 birch road boston 02101 us"),
            ("8", "505 Walnut Parkway Miami 33101 US", "505 walnut parkway miami 33101 us"),
            ("9", "123 Main Street Chicago 60602 US", "123 main street chicago 60602 us"),
            ("10", "600 Broadway Avenue New York 10001 US", "600 broadway avenue new york 10001 us"),
        ];

        for (id, label, label_norm) in entries {
            conn.execute(
                "INSERT INTO test_lookup VALUES (?, ?, ?)",
                duckdb::params![id, label, label_norm],
            )
            .unwrap();
        }
    }

    #[test]
    fn test_preprocess_address() {
        let result = preprocess_address("123 N Main St, Seattle, WA 98101");
        assert!(result.contains("north"));
        assert!(result.contains("main"));
        assert!(result.contains("street"));
        assert!(result.contains("seattle"));
        assert!(result.contains("98101"));
        // WA should be filtered as noise
        assert!(!result.contains(" wa "));
    }

    #[test]
    fn test_build_and_search_index() {
        let conn = Connection::open_in_memory().unwrap();
        create_test_lookup_table(&conn);

        let tmp_dir = format!("/tmp/spatia_tantivy_test_{}", unique_suffix());
        let index_dir = Path::new(&tmp_dir);

        // Build index
        let count = build_index(&conn, "test_lookup", index_dir).unwrap();
        assert_eq!(count, 10);

        // Search for exact match
        let results = search_addresses(index_dir, "123 Main St Springfield 62704", 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].source_id, "1");
        assert!(results[0].score > 0.5);

        // Cleanup
        let _ = std::fs::remove_dir_all(index_dir);
    }

    #[test]
    fn test_fuzzy_search_with_typo() {
        let conn = Connection::open_in_memory().unwrap();
        create_test_lookup_table(&conn);

        let tmp_dir = format!("/tmp/spatia_tantivy_typo_{}", unique_suffix());
        let index_dir = Path::new(&tmp_dir);

        build_index(&conn, "test_lookup", index_dir).unwrap();

        // "Stret" is a typo for "Street" — fuzzy should still match
        let results = search_addresses(index_dir, "123 Main Stret Springfield", 5).unwrap();
        assert!(!results.is_empty());
        // Top result should be the Springfield entry
        assert_eq!(results[0].source_id, "1");

        let _ = std::fs::remove_dir_all(index_dir);
    }

    #[test]
    fn test_abbreviation_expansion_matching() {
        let conn = Connection::open_in_memory().unwrap();
        create_test_lookup_table(&conn);

        let tmp_dir = format!("/tmp/spatia_tantivy_abbrev_{}", unique_suffix());
        let index_dir = Path::new(&tmp_dir);

        build_index(&conn, "test_lookup", index_dir).unwrap();

        // Query uses "Ave" which expands to "avenue" — should match Oak Avenue
        let results = search_addresses(index_dir, "456 Oak Ave Chicago", 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].source_id, "2");

        // Query uses "Blvd" → "boulevard"
        let results = search_addresses(index_dir, "789 Elm Blvd Seattle", 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].source_id, "3");

        let _ = std::fs::remove_dir_all(index_dir);
    }

    #[test]
    fn test_postcode_boost() {
        let conn = Connection::open_in_memory().unwrap();
        create_test_lookup_table(&conn);

        let tmp_dir = format!("/tmp/spatia_tantivy_postcode_{}", unique_suffix());
        let index_dir = Path::new(&tmp_dir);

        build_index(&conn, "test_lookup", index_dir).unwrap();

        // Two entries with "123 Main Street" — postcode should disambiguate
        let results = search_addresses(index_dir, "123 Main St 60602", 5).unwrap();
        assert!(!results.is_empty());
        // Entry 9 has postcode 60602 (Chicago), should rank first
        assert_eq!(results[0].source_id, "9");

        let _ = std::fs::remove_dir_all(index_dir);
    }

    #[test]
    fn test_empty_query_returns_empty() {
        let results = search_addresses(Path::new("/nonexistent"), "", 5).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_nonexistent_index_returns_empty() {
        let results = search_addresses(Path::new("/tmp/nonexistent_index_dir"), "test", 5).unwrap();
        assert!(results.is_empty());
    }
}
