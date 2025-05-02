#[derive(Debug, thiserror::Error)]
pub enum MergeMineError {
    #[error("Hashing of Monero data failed: {0}")]
    HashingError(String),
}
