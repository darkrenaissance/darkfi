use std::{error, fmt};

pub type Error = Box<dyn error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct MissingSpecifier;

impl fmt::Display for MissingSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "missing node specifier. you must specify either a or b")
    }
}

impl error::Error for MissingSpecifier {}
