/// Block definition
pub mod block;
pub use block::{Block, BlockInfo, BlockProposal, Header, ProposalChain};

/// Consensus metadata
pub mod metadata;
pub use metadata::{LeadProof, Metadata};

/// Consensus participant
pub mod participant;
pub use participant::{KeepAlive, Participant};

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

/// Ouroboros simulation
pub mod ouroboros;

/// Ouroboros consensus coins functions
pub mod coins;

/// Utility types
pub mod types;
pub use types::Float10;

/// Utility functions
pub mod utils;

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

    // Epoch configuration
    pub static ref EPOCH_LENGTH: u64 = 10;
    pub static ref REWARD: u64 = 420;

    // TODO: Describe constants meaning in comment
    pub static ref RADIX_BITS: usize = 76;
    pub static ref P: &'static str = "28948022309329048855892746252171976963363056481941560715954676764349967630337";
    pub static ref LOTTERY_HEAD_START: u64 = 1;
    pub static ref PRF_NULLIFIER_PREFIX: u64 = 0;

}
