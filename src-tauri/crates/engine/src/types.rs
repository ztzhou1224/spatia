use std::error::Error;

pub type EngineResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
