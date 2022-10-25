// ANCHOR: error
use std::{error, fmt};

#[derive(Debug, Clone)]
pub struct ErrorMissingSpecifier;

impl fmt::Display for ErrorMissingSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "missing node specifier. you must specify either a or b")
    }
}

impl error::Error for ErrorMissingSpecifier {}
// ANCHOR_END: error
