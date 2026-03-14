use std::error::Error;

pub type IngestResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
