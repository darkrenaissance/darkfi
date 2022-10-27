//! Encodings for external crates

#[cfg(feature = "collections")]
mod collections;

#[cfg(feature = "hash")]
mod hash;

#[cfg(feature = "incrementalmerkletree")]
mod incrementalmerkletree;

#[cfg(feature = "pasta_curves")]
mod pasta;

#[cfg(feature = "url")]
mod url;

#[cfg(feature = "x25519-dalek")]
mod x25519;
