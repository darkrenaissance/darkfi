//! This module contains a bit more minimal implementations of the
//! objects and types that can be found in `darkfi::crypto`.
//! This is done so we can have a lot less dependencies in this SDK,
//! and therefore make compilation of smart contracts faster in a sense.
//!
//! Eventually, we should strive to somehow migrate the types from
//! `darkfi::crypto` into here, and then implement certain functionality
//! in the library using traits.
//! If you feel like trying, please help out with this migration, but do
//! it properly, with care, and write documentation while you're at it.

/// Nullifier definitions
pub mod nullifier;
pub use nullifier::Nullifier;
