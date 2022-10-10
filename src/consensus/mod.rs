/// Block definition
pub mod block;
pub use block::{Block, BlockInfo, BlockProposal, Header, ProposalChain};

/// Consensus metadata
pub mod metadata;
pub use metadata::{
    OuroborosMetadata, StakeholderMetadata, StreamletMetadata, TransactionLeadProof,
};

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

/// Lamport clock
pub mod clock;
pub use clock::{Clock, Ticks};

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

    /// Block magic bytes
    pub static ref BLOCK_MAGIC_BYTES: [u8; 4] = [0x11, 0x6d, 0x75, 0x1f];

    /// Block info magic bytes
    pub static ref BLOCK_INFO_MAGIC_BYTES: [u8; 4] = [0x90, 0x44, 0xf1, 0xf6];
}
