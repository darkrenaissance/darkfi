//! # Structures
//!
//! A library for modeling consensus algorithm structures.

pub mod block;
pub mod blockchain;
pub mod node;
pub mod vote;

pub use block::Block;
pub use blockchain::Blockchain;
pub use node::Node;
pub use vote::Vote;
