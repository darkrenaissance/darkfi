pub mod cashierdb;
pub mod wallet_api;
pub mod walletdb;

#[derive(Debug, Clone, thiserror::Error)]
pub enum WalletError {
    #[error("Empty password")]
    EmptyPassword,

    #[error("Merkle tree already exists")]
    TreeExists,
}
