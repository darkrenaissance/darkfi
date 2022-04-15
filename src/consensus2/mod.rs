/// Block definition
pub mod block;
pub use block::{Block, ProposalChain};

/// Transactions
pub mod tx;
pub use tx::Tx;

/// Consensus metadata
pub mod metadata;
pub use metadata::{Metadata, StreamletMetadata};

/// Consensus participant
pub mod participant;
pub use participant::Participant;

/// Consensus vote
pub mod vote;
pub use vote::Vote;

/// Consensus state
pub mod state;
pub use state::ValidatorState;

/// Utility functions
pub mod util;

use lazy_static::lazy_static;
lazy_static! {
    /// Genesis hash for the mainnet chain
    pub static ref MAINNET_GENESIS_HASH_BYTES: [u8; 32] = *blake3::hash(b"darkfi_mainnet").as_bytes();

    /// Genesis hash for the testnet chain
    pub static ref TESTNET_GENESIS_HASH_BYTES: [u8; 32] = *blake3::hash(b"darkfi_testnet").as_bytes();
}
