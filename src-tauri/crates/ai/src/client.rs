use serde::{Deserialize, Serialize};
use tracing::{debug, error};

use crate::AiResult;

const GEMINI_API_BASE: &str =
    "https://generativelanguage.googleapis.com/v1beta/models";

/// Default Gemini model used when none is specified.
pub const DEFAULT_MODEL: &str = "gemini-2.5-flash";

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

#[derive(Serialize)]
struct GenerationConfig {
    response_mime_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize)]
struct GenerateRequestWithConfig<'a> {
    contents: Vec<Content<'a>>,
    generation_config: GenerationConfig,
}

#[derive(Serialize)]
struct TemperatureConfig {
    temperature: f32,
}

#[derive(Serialize)]
struct GenerateRequestWithTemperature<'a> {
    contents: Vec<Content<'a>>,
    generation_config: TemperatureConfig,
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
    temperature: Option<f32>,
}

impl GeminiClient {
    /// Create a client using the provided `api_key` and the default model.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
            http: reqwest::Client::new(),
            temperature: None,
        }
    }

    /// Create a client using the provided `api_key` and a custom `model` name.
    pub fn with_model(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            http: reqwest::Client::new(),
            temperature: None,
        }
    }

    /// Set the temperature for generation (0.0 = deterministic, 1.0 = creative).
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
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

    /// Send `prompt` to the Gemini `generateContent` endpoint with
    /// `response_mime_type: "application/json"` and return the first text
    /// response candidate.
    pub async fn generate_json(&self, prompt: &str) -> AiResult<String> {
        let url = format!(
            "{}/{model}:generateContent?key={key}",
            GEMINI_API_BASE,
            model = self.model,
            key = self.api_key,
        );
        // Safe URL for logging — never expose the API key.
        let log_url = format!("{}/{model}:generateContent?key=[REDACTED]", GEMINI_API_BASE, model = self.model);

        debug!(model = %self.model, prompt_len = prompt.len(), "generate_json: sending JSON-mode request to Gemini");

        let body = GenerateRequestWithConfig {
            contents: vec![Content {
                parts: vec![Part { text: prompt }],
            }],
            generation_config: GenerationConfig {
                response_mime_type: "application/json",
                temperature: self.temperature,
            },
        };

        let response = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .inspect_err(|e| {
                let redacted = e.to_string().replace(self.api_key.as_str(), "[REDACTED]");
                error!(model = %self.model, url = %log_url, error = %redacted, "generate_json: HTTP request failed");
            })?
            .error_for_status()
            .inspect_err(|e| {
                let redacted = e.to_string().replace(self.api_key.as_str(), "[REDACTED]");
                error!(model = %self.model, url = %log_url, error = %redacted, "generate_json: Gemini API returned error status");
            })?;

        let parsed: GenerateResponse = response.json().await?;

        let result = parsed
            .candidates
            .into_iter()
            .next()
            .and_then(|c| c.content.parts.into_iter().next())
            .map(|p| p.text)
            .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                "Gemini returned no text candidates".into()
            });

        if let Ok(ref text) = result {
            debug!(model = %self.model, response_len = text.len(), "generate_json: received response");
        } else {
            error!(model = %self.model, "generate_json: no candidates in response");
        }

        result
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
        // Safe URL for logging — never expose the API key.
        let log_url = format!("{}/{model}:generateContent?key=[REDACTED]", GEMINI_API_BASE, model = self.model);

        debug!(model = %self.model, prompt_len = prompt.len(), "generate: sending request to Gemini");

        let contents = vec![Content {
            parts: vec![Part { text: prompt }],
        }];

        let request_builder = if let Some(temp) = self.temperature {
            let body = GenerateRequestWithTemperature {
                contents,
                generation_config: TemperatureConfig { temperature: temp },
            };
            self.http.post(&url).json(&body)
        } else {
            let body = GenerateRequest { contents };
            self.http.post(&url).json(&body)
        };

        let response = request_builder
            .send()
            .await
            .inspect_err(|e| {
                let redacted = e.to_string().replace(self.api_key.as_str(), "[REDACTED]");
                error!(model = %self.model, url = %log_url, error = %redacted, "generate: HTTP request failed");
            })?
            .error_for_status()
            .inspect_err(|e| {
                let redacted = e.to_string().replace(self.api_key.as_str(), "[REDACTED]");
                error!(model = %self.model, url = %log_url, error = %redacted, "generate: Gemini API returned error status");
            })?;

        let parsed: GenerateResponse = response.json().await?;

        let result = parsed
            .candidates
            .into_iter()
            .next()
            .and_then(|c| c.content.parts.into_iter().next())
            .map(|p| p.text)
            .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                "Gemini returned no text candidates".into()
            });

        if let Ok(ref text) = result {
            debug!(model = %self.model, response_len = text.len(), "generate: received response");
        } else {
            error!(model = %self.model, "generate: no candidates in response");
        }

        result
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
