use serde::Deserialize;

/// A single benchmark test case loaded from TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct TestCase {
    pub name: String,
    pub description: String,
    pub setup_csv: String,
    pub setup_table: String,
    pub query: String,

    #[serde(default)]
    pub expect_sql_contains: Vec<String>,
    #[serde(default)]
    pub expect_sql_not_contains: Vec<String>,
    pub expect_row_count: Option<usize>,
    pub expect_min_rows: Option<usize>,
    #[serde(default)]
    pub expect_columns: Vec<String>,
    #[serde(default = "default_true")]
    pub expect_success: bool,

    #[serde(default)]
    pub tags: Vec<String>,
    pub timeout_secs: Option<u64>,
}

fn default_true() -> bool {
    true
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

#[cfg(test)]
mod tests {
    use super::Corpus;

    #[test]
    fn parses_minimal_test_case() {
        let toml = r#"
[[tests]]
name = "simple_count"
description = "Count rows"
setup_csv = "data/test.csv"
setup_table = "restaurants"
query = "How many restaurants are there?"
expect_row_count = 1
expect_columns = ["count"]
tags = ["simple"]
"#;
        let corpus = Corpus::from_str(toml).expect("parse");
        assert_eq!(corpus.tests.len(), 1);
        assert_eq!(corpus.tests[0].name, "simple_count");
        assert!(corpus.tests[0].expect_success);
    }

    #[test]
    fn filter_by_tags_works() {
        let toml = r#"
[[tests]]
name = "a"
description = ""
setup_csv = "x.csv"
setup_table = "t"
query = "q"
tags = ["geo"]

[[tests]]
name = "b"
description = ""
setup_csv = "x.csv"
setup_table = "t"
query = "q"
tags = ["simple"]
"#;
        let corpus = Corpus::from_str(toml).expect("parse");
        assert_eq!(corpus.filter_by_tags(&[]).len(), 2);
        assert_eq!(corpus.filter_by_tags(&["geo".to_string()]).len(), 1);
    }
}
