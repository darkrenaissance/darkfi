#![allow(unused)]

pub mod mint;
pub mod state;

pub use state::{DaoBulla, State};

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("Malformed packet")]
    MalformedPacket,
}
type Result<T> = std::result::Result<T, Error>;
