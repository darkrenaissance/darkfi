use std::fmt;

pub type Result<T> = std::result::Result<T, ServicesError>;

#[derive(Debug, Copy, Clone)]
pub enum ServicesError {
    ResonseError(&'static str),
}

impl std::error::Error for ServicesError {}

impl fmt::Display for ServicesError {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match *self {
            ServicesError::ResonseError(ref err) => write!(f, "Response: {}", err),
        }
    }
}
