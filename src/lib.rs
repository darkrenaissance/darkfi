pub mod circuit;
pub mod cli;
pub mod crypto;
pub mod net;
//pub mod rpc;
pub mod system;
pub mod types;
pub mod zk;

#[cfg(feature = "node")]
pub mod node;

#[cfg(feature = "node")]
pub mod tx;

#[cfg(feature = "tui")]
pub mod tui;

#[cfg(feature = "blockchain")]
pub mod blockchain;

pub mod util;

pub use util::{
    error,
    error::{Error, Result},
    serial,
};
