#[cfg(feature = "gemini")]
mod cleaner;
#[cfg(feature = "gemini")]
mod client;
#[cfg(feature = "gemini")]
mod prompts;

#[cfg(feature = "gemini")]
pub use cleaner::{clean_raw_staging, clean_table, CleanResult};
#[cfg(feature = "gemini")]
pub use client::{GeminiClient, DEFAULT_MODEL};
#[cfg(feature = "gemini")]
pub use prompts::{
    build_analysis_chat_system_prompt, build_analysis_sql_prompt, build_clean_prompt,
    build_visualization_command_prompt,
};

/// Shared result type for the AI crate.
pub type AiResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
