mod client;
mod cleaner;
mod prompts;

pub use client::{GeminiClient, DEFAULT_MODEL};
pub use cleaner::{clean_table, CleanResult};
pub use prompts::build_clean_prompt;

/// Shared result type for the AI crate.
pub type AiResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
