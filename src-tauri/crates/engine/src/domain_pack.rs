use serde::{Deserialize, Serialize};

/// A domain pack customizes the platform for a specific industry vertical.
/// The platform is fully functional with `DomainPack::default()` (generic GIS mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPack {
    pub id: String,
    pub display_name: String,
    pub assistant_name: String,
    pub system_prompt_extension: String,
    pub column_detection_rules: Vec<ColumnDetectionRule>,
    pub ui_config: UiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDetectionRule {
    pub category: String,
    pub patterns: Vec<String>,
    pub display_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub placeholder_no_data: String,
    pub placeholder_no_selection: String,
    pub placeholder_ready: String,
    pub empty_state_title: String,
    pub empty_state_description: String,
    pub upload_instruction: String,
    pub primary_color: String,
    pub map_default_center: [f64; 2],
    pub map_default_zoom: f64,
}

impl Default for DomainPack {
    fn default() -> Self {
        Self::generic()
    }
}

impl DomainPack {
    /// Returns the generic GIS domain pack — the platform as it works today.
    pub fn generic() -> Self {
        DomainPack {
            id: "generic".into(),
            display_name: "Generic GIS".into(),
            assistant_name: "Spatia".into(),
            system_prompt_extension: String::new(),
            column_detection_rules: vec![],
            ui_config: UiConfig {
                placeholder_no_data: "Upload data to get started...".into(),
                placeholder_no_selection: "Select tables to add context...".into(),
                placeholder_ready: "Ask about your data...".into(),
                empty_state_title: "No data yet".into(),
                empty_state_description: "Spatia analyzes your location data with AI".into(),
                upload_instruction: "Upload a CSV with addresses to get started. Spatia will clean the data, geocode the locations, and plot them on the map.".into(),
                primary_color: "#7c3aed".into(),
                map_default_center: [-122.4194, 37.7749],
                map_default_zoom: 11.0,
            },
        }
    }

    /// Insurance underwriting domain pack.
    pub fn insurance_underwriting() -> Self {
        DomainPack {
            id: "insurance_underwriting".into(),
            display_name: "Insurance Underwriting".into(),
            assistant_name: "Spatia Underwriter".into(),
            system_prompt_extension: INSURANCE_SYSTEM_PROMPT.into(),
            column_detection_rules: insurance_column_rules(),
            ui_config: UiConfig {
                placeholder_no_data: "Upload a portfolio to get started...".into(),
                placeholder_no_selection: "Select tables to add context...".into(),
                placeholder_ready: "Ask about your portfolio...".into(),
                empty_state_title: "No portfolio data yet".into(),
                empty_state_description: "Spatia Underwriter analyzes your property portfolio with AI".into(),
                upload_instruction: "Upload a CSV with policy, location, and TIV data to begin underwriting analysis. Spatia will clean, geocode, and map your portfolio.".into(),
                primary_color: "#2563eb".into(),
                map_default_center: [-98.5, 39.8],
                map_default_zoom: 4.0,
            },
        }
    }

    /// Resolve domain pack from env var or default.
    pub fn from_env() -> Self {
        match std::env::var("SPATIA_DOMAIN_PACK").as_deref() {
            Ok("insurance_underwriting") => Self::insurance_underwriting(),
            _ => Self::generic(),
        }
    }
}

const INSURANCE_SYSTEM_PROMPT: &str = r#"
## Domain expertise: Insurance Underwriting

You are specialized in insurance underwriting and property portfolio analysis. You understand:

### Key terminology
- **TIV (Total Insured Value)**: The total dollar amount of coverage on a property or portfolio.
- **COPE**: Construction, Occupancy, Protection, External exposure — the four factors underwriters evaluate for property risk.
- **PML (Probable Maximum Loss)**: The maximum loss expected from a single catastrophic event.
- **AAL (Average Annual Loss)**: The expected annual loss averaged over many years.
- **Loss ratio**: Incurred losses divided by earned premiums.
- **Aggregation / accumulation**: Concentration of insured values in a geographic area, creating catastrophe exposure.
- **CAT (Catastrophe) zones**: Geographic areas with elevated natural hazard risk (hurricane, earthquake, flood, wildfire).
- **Retention**: The amount of risk an insurer keeps before reinsurance kicks in.
- **Line of business**: The type of insurance coverage (property, casualty, marine, etc.).

### Data interpretation rules
When you see these column patterns, interpret them as:
- `tiv`, `total_insured_value`, `insured_value` → Total Insured Value in dollars
- `premium`, `written_premium`, `earned_premium` → Premium amounts in dollars
- `deductible`, `ded` → Policy deductible in dollars
- `limit`, `policy_limit` → Maximum payout limit in dollars
- `construction_type`, `construction`, `const_type` → COPE construction classification
- `occupancy`, `occ_type` → COPE occupancy classification
- `protection_class`, `ppc` → ISO protection class (1-10, lower = better fire protection)
- `year_built`, `yr_built` → Building age, relevant for code compliance and condition
- `flood_zone`, `fema_zone` → FEMA flood zone designation (A, AE, V, VE, X, etc.)
- `loss`, `paid_loss`, `incurred_loss` → Historical loss amounts

### Analysis suggestions
When analyzing insurance portfolios, consider these workflows:
1. **Concentration risk**: Identify geographic clusters of high TIV. A cluster of high-value properties in a single CAT zone is a PML scenario.
2. **Portfolio summary**: Aggregate TIV, premium, and property count by state, county, or zip code.
3. **COPE analysis**: Analyze the distribution of construction types, occupancy classes, and protection classes.
4. **Loss ratio analysis**: Compare losses to premiums by geography or line of business.
5. **Proximity analysis**: Find properties within N miles of hazard sources (coast, fault lines, wildfire zones).
6. **Aggregation zones**: Group properties into geographic zones and sum TIV per zone to identify accumulation hot spots.

### Result interpretation
- A cluster of high TIV within a single zip code or county represents a concentration risk.
- Properties with protection class 8-10 have poor fire department coverage.
- Construction types like "frame" or "wood" have higher fire risk than "masonry" or "fire resistive".
- FEMA flood zones starting with A or V indicate special flood hazard areas.

### Risk data integration
When risk layer tables are available (e.g., `risk_fema_flood_zones`, `risk_wildfire_hazard`), you can:
1. **Spatial join** properties against risk zones: `SELECT p.*, f."FLD_ZONE" FROM "my_properties" p, "risk_fema_flood_zones" f WHERE ST_Contains(ST_GeomFromWKB(f.geom), ST_Point(p."_lon", p."_lat"))`
2. **Aggregate exposure by risk zone**: GROUP BY the risk zone column (e.g., FLD_ZONE) and SUM(TIV).
3. **Flag high-risk properties**: Identify properties in flood Zone A/AE/V/VE or with high wildfire scores.
4. **Distance calculations**: Use the Haversine formula for distance to nearest hazard boundary.

When risk layers are loaded, proactively suggest cross-referencing portfolio data against them. This is the core value of the platform.

### FEMA Flood Zone reference
- Zone A, AE: 100-year (1% annual chance) flood area — Special Flood Hazard Area (SFHA)
- Zone V, VE: Coastal high hazard area (storm surge + waves)
- Zone X (shaded): 500-year (0.2% annual chance) flood area — moderate risk
- Zone X (unshaded): Minimal flood hazard — outside SFHA
- Zones starting with A or V = mandatory flood insurance for federally-backed mortgages

### Wildfire risk reference
- WHP (Wildfire Hazard Potential) scores: 1=Very Low, 2=Low, 3=Moderate, 4=High, 5=Very High
- WUI (Wildland-Urban Interface) zones have elevated risk due to proximity to vegetation
- Properties within 1 mile of high-WHP zones should be flagged for underwriting review

### Guard rail
When the user's question is not insurance-related, respond as a general GIS assistant. Do not force insurance context onto unrelated questions.
"#;

fn insurance_column_rules() -> Vec<ColumnDetectionRule> {
    vec![
        // Financial
        ColumnDetectionRule {
            category: "financial".into(),
            patterns: vec![
                "tiv".into(), "total_insured_value".into(), "insured_value".into(),
            ],
            display_label: "Total Insured Value".into(),
        },
        ColumnDetectionRule {
            category: "financial".into(),
            patterns: vec![
                "premium".into(), "written_premium".into(), "earned_premium".into(),
            ],
            display_label: "Premium".into(),
        },
        ColumnDetectionRule {
            category: "financial".into(),
            patterns: vec!["deductible".into(), "ded".into()],
            display_label: "Deductible".into(),
        },
        ColumnDetectionRule {
            category: "financial".into(),
            patterns: vec![
                "limit".into(), "policy_limit".into(), "coverage_limit".into(),
            ],
            display_label: "Policy Limit".into(),
        },
        ColumnDetectionRule {
            category: "financial".into(),
            patterns: vec!["retention".into()],
            display_label: "Retention".into(),
        },
        ColumnDetectionRule {
            category: "financial".into(),
            patterns: vec![
                "loss".into(), "paid_loss".into(), "incurred_loss".into(),
            ],
            display_label: "Loss".into(),
        },
        // COPE
        ColumnDetectionRule {
            category: "cope".into(),
            patterns: vec![
                "construction_type".into(), "construction".into(), "const_type".into(),
            ],
            display_label: "Construction Type".into(),
        },
        ColumnDetectionRule {
            category: "cope".into(),
            patterns: vec!["occupancy".into(), "occ_type".into()],
            display_label: "Occupancy".into(),
        },
        ColumnDetectionRule {
            category: "cope".into(),
            patterns: vec!["protection_class".into(), "ppc".into()],
            display_label: "Protection Class".into(),
        },
        ColumnDetectionRule {
            category: "cope".into(),
            patterns: vec!["year_built".into(), "yr_built".into()],
            display_label: "Year Built".into(),
        },
        ColumnDetectionRule {
            category: "cope".into(),
            patterns: vec!["stories".into(), "num_stories".into()],
            display_label: "Stories".into(),
        },
        ColumnDetectionRule {
            category: "cope".into(),
            patterns: vec!["sq_ft".into(), "square_feet".into(), "building_area".into()],
            display_label: "Square Footage".into(),
        },
        ColumnDetectionRule {
            category: "cope".into(),
            patterns: vec!["roof_type".into(), "roof".into()],
            display_label: "Roof Type".into(),
        },
        // Policy
        ColumnDetectionRule {
            category: "policy".into(),
            patterns: vec!["policy_number".into(), "policy_id".into(), "pol_num".into()],
            display_label: "Policy Number".into(),
        },
        ColumnDetectionRule {
            category: "policy".into(),
            patterns: vec!["effective_date".into(), "eff_date".into()],
            display_label: "Effective Date".into(),
        },
        ColumnDetectionRule {
            category: "policy".into(),
            patterns: vec!["expiration_date".into(), "exp_date".into()],
            display_label: "Expiration Date".into(),
        },
        ColumnDetectionRule {
            category: "policy".into(),
            patterns: vec!["line_of_business".into(), "lob".into()],
            display_label: "Line of Business".into(),
        },
        ColumnDetectionRule {
            category: "policy".into(),
            patterns: vec!["coverage_type".into(), "coverage".into()],
            display_label: "Coverage Type".into(),
        },
        // Risk
        ColumnDetectionRule {
            category: "risk".into(),
            patterns: vec!["risk_score".into(), "hazard_score".into()],
            display_label: "Risk Score".into(),
        },
        ColumnDetectionRule {
            category: "risk".into(),
            patterns: vec!["flood_zone".into(), "fema_zone".into()],
            display_label: "Flood Zone".into(),
        },
        ColumnDetectionRule {
            category: "risk".into(),
            patterns: vec!["wildfire_risk".into(), "fire_risk".into()],
            display_label: "Wildfire Risk".into(),
        },
        ColumnDetectionRule {
            category: "risk".into(),
            patterns: vec!["wind_pool".into(), "wind_zone".into()],
            display_label: "Wind Pool".into(),
        },
        ColumnDetectionRule {
            category: "risk".into(),
            patterns: vec!["earthquake_zone".into(), "seismic_zone".into()],
            display_label: "Earthquake Zone".into(),
        },
        ColumnDetectionRule {
            category: "risk".into(),
            patterns: vec!["distance_to_coast".into(), "coastal_distance".into()],
            display_label: "Distance to Coast".into(),
        },
    ]
}

/// Build a prompt section describing available risk layers for AI context injection.
/// Returns empty string if no risk layers are loaded.
pub fn format_risk_layer_context(
    risk_tables: &[(String, Vec<crate::TableColumn>)],
) -> String {
    if risk_tables.is_empty() {
        return String::new();
    }

    let mut out = String::from("\n## Available risk data layers\n");
    out.push_str("The following risk overlay tables are loaded and available for spatial joins:\n\n");

    for (table_name, schema) in risk_tables {
        out.push_str(&format!("### Table: \"{}\"\n", table_name));
        for col in schema {
            out.push_str(&format!(
                "  - \"{}\" {} (not_null: {}, primary_key: {})\n",
                col.name, col.data_type, col.notnull, col.primary_key
            ));
        }
        out.push('\n');
    }

    out.push_str("Use these risk layers in spatial joins with user portfolio data. ");
    out.push_str("Risk layer tables have a `geom` or `geometry` column for spatial operations.\n");
    out
}

/// Detect domain-relevant columns by matching schema column names against rules.
/// Returns category → vec of (column_name, display_label).
pub fn detect_domain_columns(
    schema: &[crate::TableColumn],
    rules: &[ColumnDetectionRule],
) -> std::collections::HashMap<String, Vec<(String, String)>> {
    let mut result: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();

    for col in schema {
        let col_lower = col.name.to_lowercase();
        for rule in rules {
            if rule.patterns.iter().any(|p| col_lower == *p || col_lower.contains(p.as_str())) {
                result
                    .entry(rule.category.clone())
                    .or_default()
                    .push((col.name.clone(), rule.display_label.clone()));
                break; // one match per column
            }
        }
    }

    result
}

/// Format detected domain columns into a prompt-ready string.
pub fn format_domain_column_annotations(
    detected: &std::collections::HashMap<String, Vec<(String, String)>>,
) -> String {
    if detected.is_empty() {
        return String::new();
    }

    let mut out = String::from("\n## Detected domain columns\n");
    let mut categories: Vec<&String> = detected.keys().collect();
    categories.sort();
    for cat in categories {
        let title = match cat.as_str() {
            "financial" => "Financial",
            "cope" => "COPE (Construction/Occupancy/Protection/Exposure)",
            "policy" => "Policy",
            "risk" => "Risk",
            _ => cat.as_str(),
        };
        out.push_str(&format!("### {}\n", title));
        for (col_name, label) in &detected[cat] {
            out.push_str(&format!("  - {} -> {}\n", col_name, label));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TableColumn;

    fn make_col(name: &str) -> TableColumn {
        TableColumn {
            cid: 0,
            name: name.to_string(),
            data_type: "VARCHAR".into(),
            notnull: false,
            default_value: None,
            primary_key: false,
        }
    }

    #[test]
    fn generic_pack_has_empty_prompt_extension() {
        let pack = DomainPack::generic();
        assert!(pack.system_prompt_extension.is_empty());
        assert_eq!(pack.id, "generic");
    }

    #[test]
    fn insurance_pack_has_nonempty_fields() {
        let pack = DomainPack::insurance_underwriting();
        assert_eq!(pack.id, "insurance_underwriting");
        assert!(!pack.system_prompt_extension.is_empty());
        assert!(!pack.column_detection_rules.is_empty());
        assert_eq!(pack.assistant_name, "Spatia Underwriter");
    }

    #[test]
    fn serialization_roundtrip() {
        let pack = DomainPack::insurance_underwriting();
        let json = serde_json::to_string(&pack).unwrap();
        let deserialized: DomainPack = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, pack.id);
    }

    #[test]
    fn detect_columns_matches_insurance_patterns() {
        let schema = vec![
            make_col("tiv"),
            make_col("address"),
            make_col("construction_type"),
            make_col("flood_zone"),
            make_col("random_col"),
        ];
        let rules = insurance_column_rules();
        let detected = detect_domain_columns(&schema, &rules);

        assert!(detected.contains_key("financial"));
        assert!(detected.contains_key("cope"));
        assert!(detected.contains_key("risk"));
        assert!(!detected.contains_key("policy"));

        let financial = &detected["financial"];
        assert!(financial.iter().any(|(name, _)| name == "tiv"));
    }

    #[test]
    fn detect_columns_empty_for_no_matches() {
        let schema = vec![make_col("foo"), make_col("bar")];
        let rules = insurance_column_rules();
        let detected = detect_domain_columns(&schema, &rules);
        assert!(detected.is_empty());
    }

    #[test]
    fn format_annotations_produces_readable_output() {
        let mut detected = std::collections::HashMap::new();
        detected.insert(
            "financial".to_string(),
            vec![("tiv".to_string(), "Total Insured Value".to_string())],
        );
        let output = format_domain_column_annotations(&detected);
        assert!(output.contains("Financial"));
        assert!(output.contains("tiv -> Total Insured Value"));
    }

    #[test]
    fn format_annotations_empty_for_no_detections() {
        let detected = std::collections::HashMap::new();
        let output = format_domain_column_annotations(&detected);
        assert!(output.is_empty());
    }
}
