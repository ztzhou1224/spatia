use std::collections::HashSet;

use crate::text::{expand_abbreviation, is_noise_token_smart};

/// Absolute floor for even considering a local fuzzy candidate (inclusive).
/// A score below this threshold means the candidate is so unlike the query
/// that it is discarded entirely.
pub const MIN_SCORE: f64 = 0.45;

/// Default minimum score for a local fuzzy match to be *accepted* as a
/// resolved result.  Matches that score at or above `MIN_SCORE` but below
/// this threshold are considered too low-quality to accept locally; they are
/// returned to the unresolved pool so that the Geocodio API fallback can
/// attempt a proper geocode.
///
/// Override at runtime via the `SPATIA_LOCAL_GEOCODE_MIN_CONFIDENCE` env var.
pub const MIN_LOCAL_ACCEPT_SCORE: f64 = 0.75;

/// Read the local-accept confidence threshold from the environment, falling
/// back to `MIN_LOCAL_ACCEPT_SCORE` if the variable is absent or unparseable.
pub fn local_accept_threshold() -> f64 {
    std::env::var("SPATIA_LOCAL_GEOCODE_MIN_CONFIDENCE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(MIN_LOCAL_ACCEPT_SCORE)
}

/// Normalize an address string for scoring: expand abbreviations and remove
/// noise tokens (state codes, country codes). Used only in `score_candidate`,
/// NOT in the SQL LIKE pre-filter which needs raw tokens.
pub(crate) fn normalize_for_scoring(address_norm: &str) -> Vec<String> {
    address_norm
        .split_whitespace()
        .map(|t| (t, expand_abbreviation(t)))
        .filter(|(orig, expanded)| !is_noise_token_smart(orig, expanded))
        .map(|(_, expanded)| expanded.to_string())
        .collect()
}

pub fn score_candidate(query_norm: &str, label_norm: &str) -> f64 {
    if query_norm.is_empty() || label_norm.is_empty() {
        return 0.0;
    }

    // Normalize both sides: expand abbreviations and strip noise tokens
    let q_tokens = normalize_for_scoring(query_norm);
    let l_tokens = normalize_for_scoring(label_norm);

    if q_tokens.is_empty() {
        return 0.0;
    }

    let l_set: HashSet<&str> = l_tokens.iter().map(|s| s.as_str()).collect();

    // (a) Token overlap ratio — weight 0.60
    let overlap_count = q_tokens
        .iter()
        .filter(|t| l_set.contains(t.as_str()))
        .count() as f64;
    let token_overlap = overlap_count / q_tokens.len() as f64;

    // (b) Leading sequence bonus — consecutive matching tokens from start — weight 0.25
    let leading_matches = q_tokens
        .iter()
        .zip(l_tokens.iter())
        .take_while(|(q, l)| q == l)
        .count() as f64;
    let leading_ratio = leading_matches / q_tokens.len() as f64;

    // (c) Postcode match bonus — if any 5+ digit numeric token in query matches label
    let postcode_bonus = if q_tokens.iter().any(|t| {
        t.len() >= 5 && t.chars().all(|c| c.is_ascii_digit()) && l_set.contains(t.as_str())
    }) {
        0.10
    } else {
        0.0
    };

    // (d) Street number match — if first numeric token in query matches first numeric in label
    let first_numeric = |tokens: &[String]| -> Option<String> {
        tokens
            .iter()
            .find(|t| t.chars().all(|c| c.is_ascii_digit()))
            .cloned()
    };
    let street_num_bonus = match (first_numeric(&q_tokens), first_numeric(&l_tokens)) {
        (Some(q), Some(l)) if q == l => 0.05,
        _ => 0.0,
    };

    let score = (token_overlap * 0.60) + (leading_ratio * 0.25) + postcode_bonus + street_num_bonus;
    score.clamp(0.0, 0.99)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- normalize_for_scoring tests ----

    #[test]
    fn normalize_for_scoring_expands_and_filters() {
        let tokens = normalize_for_scoring("85 pike st seattle wa 98101");
        assert_eq!(tokens, vec!["85", "pike", "street", "seattle", "98101"]);
    }

    #[test]
    fn normalize_for_scoring_label_with_country_code() {
        let tokens = normalize_for_scoring("85 pike street seattle 98101 us");
        assert_eq!(tokens, vec!["85", "pike", "street", "seattle", "98101"]);
    }

    // ---- score_candidate tests ----

    #[test]
    fn score_candidate_empty_inputs() {
        assert_eq!(score_candidate("", "anything"), 0.0);
        assert_eq!(score_candidate("anything", ""), 0.0);
        assert_eq!(score_candidate("", ""), 0.0);
    }

    #[test]
    fn score_candidate_exact_match_after_normalization() {
        // "85 pike st seattle wa 98101" vs "85 pike street seattle 98101 us"
        // After normalization both become ["85", "pike", "street", "seattle", "98101"]
        let score = score_candidate(
            "85 pike st seattle wa 98101",
            "85 pike street seattle 98101 us",
        );
        assert!(
            score >= 0.90,
            "pike st vs pike street should score high, got {score:.3}"
        );
    }

    #[test]
    fn score_candidate_abbreviation_heavy() {
        // "100 n main ave" vs "100 north main avenue"
        let score = score_candidate("100 n main ave", "100 north main avenue");
        assert!(
            score >= 0.85,
            "abbreviation-heavy address should score high, got {score:.3}"
        );
    }

    #[test]
    fn score_candidate_wrong_city_rejected() {
        // Same street, wrong city — should be below 0.75
        let score = score_candidate(
            "85 pike st portland or 97201",
            "85 pike street seattle 98101 us",
        );
        assert!(
            score < MIN_LOCAL_ACCEPT_SCORE,
            "wrong city should be rejected, got {score:.3}"
        );
    }

    #[test]
    fn score_candidate_missing_zip() {
        // Query without zip vs label with zip
        let score = score_candidate(
            "85 pike st seattle",
            "85 pike street seattle 98101 us",
        );
        // Should still score well — all query tokens match
        assert!(
            score >= 0.75,
            "missing zip should still score well, got {score:.3}"
        );
    }

    #[test]
    fn score_candidate_blvd_vs_boulevard() {
        let score = score_candidate(
            "200 aurora blvd seattle wa 98133",
            "200 aurora boulevard seattle 98133 us",
        );
        assert!(
            score >= 0.90,
            "blvd vs boulevard should score high, got {score:.3}"
        );
    }

    #[test]
    fn score_candidate_different_street_number() {
        // Different street number — partial overlap but should be low
        let score = score_candidate(
            "200 pike st seattle wa 98101",
            "85 pike street seattle 98101 us",
        );
        // Tokens: [pike, street, seattle, 98101] overlap, but street number differs
        // and leading sequence breaks at position 0
        assert!(
            score < MIN_LOCAL_ACCEPT_SCORE,
            "different street number should be rejected, got {score:.3}"
        );
    }

    #[test]
    fn local_accept_threshold_defaults_to_constant() {
        std::env::remove_var("SPATIA_LOCAL_GEOCODE_MIN_CONFIDENCE");
        assert!(
            (local_accept_threshold() - MIN_LOCAL_ACCEPT_SCORE).abs() < 1e-9,
            "default threshold should equal MIN_LOCAL_ACCEPT_SCORE"
        );
    }
}
