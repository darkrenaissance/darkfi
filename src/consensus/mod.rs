/// Block definition
pub mod block;
pub use block::{Block, BlockInfo, BlockProposal, ProposalChain};

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
pub use state::{ValidatorState, ValidatorStatePtr};

/// Utility functions and types
use crate::util::time::Timestamp;

/// P2P net protocols
pub mod proto;

/// async tasks to utilize the protocols
pub mod task;

use lazy_static::lazy_static;
lazy_static! {
    /// Genesis hash for the mainnet chain
    pub static ref MAINNET_GENESIS_HASH_BYTES: blake3::Hash = blake3::hash(b"darkfi_mainnet");

    /// Genesis timestamp for the mainnet chain
    pub static ref MAINNET_GENESIS_TIMESTAMP: Timestamp = Timestamp(1650887115);

    /// Genesis hash for the testnet chain
    pub static ref TESTNET_GENESIS_HASH_BYTES: blake3::Hash = blake3::hash(b"darkfi_testnet");

    /// Genesis timestamp for the testnet chain
    pub static ref TESTNET_GENESIS_TIMESTAMP: Timestamp = Timestamp(1650887115);

    /// Block version number
    pub static ref BLOCK_VERSION: u8 = 1;
}
