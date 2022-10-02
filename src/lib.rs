pub mod error;
pub use error::{ClientFailed, ClientResult, Error, Result, VerifyFailed, VerifyResult};

#[cfg(feature = "blockchain")]
pub mod blockchain;

#[cfg(feature = "blockchain")]
pub mod stakeholder;

#[cfg(feature = "blockchain")]
pub mod consensus;

#[cfg(feature = "crypto")]
pub mod crypto;

#[cfg(feature = "crypto")]
pub mod zk;

#[cfg(feature = "dht")]
pub mod dht;

#[cfg(feature = "net")]
pub mod net;

#[cfg(feature = "node")]
pub mod node;

#[cfg(feature = "raft")]
pub mod raft;

#[cfg(feature = "rpc")]
pub mod rpc;

#[cfg(feature = "serial")]
pub mod serial;

#[cfg(feature = "system")]
pub mod system;

#[cfg(feature = "tx")]
pub mod tx;

#[cfg(feature = "util")]
pub mod util;

#[cfg(feature = "wallet")]
pub mod wallet;

#[cfg(feature = "wasm-runtime")]
pub mod runtime;

#[cfg(feature = "zkas")]
pub mod zkas;

pub const ANSI_LOGO: &str = include_str!("../contrib/darkfi.ansi");

#[macro_export]
macro_rules! cli_desc {
    () => {{
        let mut desc = env!("CARGO_PKG_DESCRIPTION").to_string();
        desc.push_str("\n");
        desc.push_str(darkfi::ANSI_LOGO);
        Box::leak(desc.into_boxed_str()) as &'static str
    }};
}
