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
