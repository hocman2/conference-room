use std::{result, error};

pub type Result<T> = result::Result<T, Error>;
pub type Error = Box<dyn error::Error + Send + Sync>;
