pub mod error;
pub use error::{ClientFailed, ClientResult, Error, Result, VerifyFailed, VerifyResult};

#[cfg(feature = "blockchain")]
pub mod blockchain;

#[cfg(feature = "blockchain")]
pub mod consensus;

#[cfg(feature = "crypto")]
pub mod crypto;

#[cfg(feature = "dht")]
pub mod dht;

#[cfg(feature = "crypto")]
pub mod zk;

#[cfg(feature = "net")]
pub mod net;

#[cfg(feature = "node")]
pub mod node;

//#[cfg(feature = "wasm-runtime")]
//pub mod runtime;

#[cfg(feature = "raft")]
pub mod raft;

#[cfg(feature = "rpc")]
pub mod rpc;

#[cfg(feature = "system")]
pub mod system;

#[cfg(feature = "tx")]
pub mod tx;

#[cfg(feature = "util")]
pub mod util;

#[cfg(feature = "wallet")]
pub mod wallet;

#[cfg(feature = "zkas")]
pub mod zkas;
