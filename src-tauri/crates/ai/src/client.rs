use serde::{Deserialize, Serialize};

use crate::AiResult;

const GEMINI_API_BASE: &str =
    "https://generativelanguage.googleapis.com/v1beta/models";

/// Default Gemini model used when none is specified.
pub const DEFAULT_MODEL: &str = "gemini-2.0-flash";

// ── Request / response shapes ────────────────────────────────────────────────

#[derive(Serialize)]
struct GenerateRequest<'a> {
    contents: Vec<Content<'a>>,
}

#[derive(Serialize)]
struct Content<'a> {
    parts: Vec<Part<'a>>,
}

#[derive(Serialize)]
struct Part<'a> {
    text: &'a str,
}

#[derive(Deserialize)]
struct GenerateResponse {
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct Candidate {
    content: ResponseContent,
}

#[derive(Deserialize)]
struct ResponseContent {
    parts: Vec<ResponsePart>,
}

#[derive(Deserialize)]
struct ResponsePart {
    text: String,
}

// ── Client ───────────────────────────────────────────────────────────────────

/// A thin async client for the Gemini `generateContent` REST endpoint.
///
/// Construct via [`GeminiClient::new`] or [`GeminiClient::with_model`].
/// The API key is read from the `SPATIA_GEMINI_API_KEY` environment variable
/// by default, or supplied explicitly.
#[derive(Debug, Clone)]
pub struct GeminiClient {
    api_key: String,
    model: String,
    http: reqwest::Client,
}

impl GeminiClient {
    /// Create a client using the provided `api_key` and the default model.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Create a client using the provided `api_key` and a custom `model` name.
    pub fn with_model(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            http: reqwest::Client::new(),
        }
    }

    /// Try to build a client from the `SPATIA_GEMINI_API_KEY` environment
    /// variable.  Returns `Err` if the variable is absent or empty.
    pub fn from_env() -> AiResult<Self> {
        let key = std::env::var("SPATIA_GEMINI_API_KEY")
            .map_err(|_| "SPATIA_GEMINI_API_KEY environment variable is not set")?;
        if key.trim().is_empty() {
            return Err("SPATIA_GEMINI_API_KEY is set but empty".into());
        }
        Ok(Self::new(key))
    }

    /// Return the model name this client is configured to use.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Send `prompt` to the Gemini `generateContent` endpoint and return the
    /// first text response candidate.
    pub async fn generate(&self, prompt: &str) -> AiResult<String> {
        let url = format!(
            "{}/{model}:generateContent?key={key}",
            GEMINI_API_BASE,
            model = self.model,
            key = self.api_key,
        );
        // Note: the Gemini REST API requires the key as a query parameter (?key=…).
        // This is the only supported authentication method for the v1beta endpoint.

        let body = GenerateRequest {
            contents: vec![Content {
                parts: vec![Part { text: prompt }],
            }],
        };

        let response = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let parsed: GenerateResponse = response.json().await?;

        parsed
            .candidates
            .into_iter()
            .next()
            .and_then(|c| c.content.parts.into_iter().next())
            .map(|p| p.text)
            .ok_or_else(|| "Gemini returned no text candidates".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{GeminiClient, DEFAULT_MODEL};

    #[test]
    fn default_model_is_set() {
        let client = GeminiClient::new("test_key");
        assert_eq!(client.model(), DEFAULT_MODEL);
    }

    #[test]
    fn with_model_overrides_default() {
        let client = GeminiClient::with_model("test_key", "gemini-1.5-pro");
        assert_eq!(client.model(), "gemini-1.5-pro");
    }

    #[test]
    fn from_env_errors_when_var_missing() {
        // Remove the key if it happens to be set in the test environment.
        std::env::remove_var("SPATIA_GEMINI_API_KEY");
        assert!(GeminiClient::from_env().is_err());
    }

    #[test]
    fn from_env_errors_when_var_empty() {
        std::env::set_var("SPATIA_GEMINI_API_KEY", "  ");
        assert!(GeminiClient::from_env().is_err());
        std::env::remove_var("SPATIA_GEMINI_API_KEY");
    }
}
