use std::sync::LazyLock;

use regex::Regex;

static ZIP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{5})(?:-\d{4})?\b").unwrap());

/// Parsed address components extracted from a raw address string or columns.
#[derive(Debug, Clone, Default)]
pub struct AddressComponents {
    pub number: Option<String>,
    pub street: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub zip: Option<String>,
    pub full: String,
}

/// Build `AddressComponents` from explicit CSV columns.
pub fn components_from_columns(
    street: &str,
    city: Option<&str>,
    state: Option<&str>,
    zip: Option<&str>,
) -> AddressComponents {
    let full = [Some(street), city, state, zip]
        .iter()
        .filter_map(|s| {
            let v = (*s)?.trim();
            if v.is_empty() { None } else { Some(v.to_string()) }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let validated_zip = zip
        .and_then(|z| extract_zip(z))
        .or_else(|| extract_zip(&full));

    let validated_state = state
        .map(|s| s.trim().to_uppercase())
        .filter(|s| US_STATE_ABBREVS.contains(&s.to_lowercase().as_str()));

    // Try to extract street number from the street column.
    let tokens = tokenize_address(street);
    let number = tokens
        .first()
        .filter(|t| t.chars().all(|c| c.is_ascii_digit()))
        .cloned();

    AddressComponents {
        number,
        street: Some(street.trim().to_string()),
        city: city.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()),
        state: validated_state,
        zip: validated_zip,
        full,
    }
}

/// Try to build `AddressComponents` from a single free-text address string.
pub fn components_from_string(address: &str) -> AddressComponents {
    let full = address.trim().to_string();
    let zip = extract_zip(&full);

    // Try to find a 2-letter state code near the end of the address.
    let tokens = tokenize_address(&full);
    let state = tokens
        .iter()
        .rev()
        .take(4) // state code is usually near the end
        .find(|t| t.len() == 2 && US_STATE_ABBREVS.contains(&t.as_str()))
        .map(|s| s.to_uppercase());

    let number = tokens
        .first()
        .filter(|t| t.chars().all(|c| c.is_ascii_digit()))
        .cloned();

    AddressComponents {
        number,
        street: Some(full.clone()),
        city: None, // not easily extractable from free text
        state,
        zip,
        full,
    }
}

/// Extract a 5-digit US zip code from an address string.
/// Handles zip+4 format (e.g., "33603-1234" → "33603").
pub fn extract_zip(text: &str) -> Option<String> {
    ZIP_RE.captures(text).map(|c| c[1].to_string())
}

/// Normalize an address string: strip non-alphanumeric chars, lowercase, collapse whitespace.
pub fn normalize_address(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut last_space = true;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }

    out.trim().to_string()
}

pub fn tokenize_address(value: &str) -> Vec<String> {
    normalize_address(value)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// Expand common street-type, directional, and unit abbreviations to their
/// full form so that "st" matches "street", "ave" matches "avenue", etc.
pub fn expand_abbreviation(token: &str) -> &str {
    match token {
        // Street types
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
        // Directionals
        "n" => "north",
        "s" => "south",
        "e" => "east",
        "w" => "west",
        "ne" => "northeast",
        "nw" => "northwest",
        "se" => "southeast",
        "sw" => "southwest",
        // Unit
        "apt" => "apartment",
        "ste" => "suite",
        "fl" => "floor",
        other => other,
    }
}

/// Two-letter US state abbreviation lookup (used by `is_noise_token_smart`).
pub const US_STATE_ABBREVS: &[&str] = &[
    "al", "ak", "az", "ar", "ca", "co", "ct", "de", "fl", "ga",
    "hi", "id", "il", "in", "ia", "ks", "ky", "la", "me", "md",
    "ma", "mi", "mn", "ms", "mo", "mt", "ne", "nv", "nh", "nj",
    "nm", "ny", "nc", "nd", "oh", "ok", "or", "pa", "ri", "sc",
    "sd", "tn", "tx", "ut", "vt", "va", "wa", "wv", "wi", "wy",
    "dc",
];

/// Smart noise detection that avoids collisions with street abbreviations.
/// Some state abbreviations like "ct" (Connecticut), "fl" (Florida),
/// "ne" (Nebraska) collide with street type abbreviations (court, floor,
/// northeast). We handle this by expanding abbreviations first and then
/// checking if the *original* token is a state/country code that was NOT
/// expanded (meaning it wasn't a street abbreviation).
pub fn is_noise_token_smart(original: &str, expanded: &str) -> bool {
    // If the token was expanded by expand_abbreviation, it's a street type,
    // not a state code — keep it.
    if original != expanded {
        return false;
    }
    // Check against state abbreviations and country codes
    US_STATE_ABBREVS.contains(&original) || matches!(original, "us" | "usa")
}

/// Simple noise check (used by search_index).
pub fn is_noise_token(token: &str) -> bool {
    US_STATE_ABBREVS.contains(&token) || matches!(token, "us" | "usa")
}

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
