use std::error::Error;

pub type OvertureResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
