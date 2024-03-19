//! This module contains the protocol traits and logic for DRK-ETH atomic swaps.
mod error;
mod follower;
pub(crate) mod initiator;
pub(crate) mod traits;

pub use error::Error;
