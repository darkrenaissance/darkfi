#[cfg(feature = "async-runtime")]
/// async utility functions
pub mod async_util;

/// Command-line interface utilities
pub mod cli;

/// Lamport clock (TODO: maybe shouldn't be in util module)
pub mod clock;
pub use clock::{Clock, Ticks};

/// Various encoding formats
pub mod encoding;

/// Filesystem utilities
pub mod file;

/// Network differentiations (TODO: shouldn't be here in util module))
pub mod net_name;

/// Parsing helpers
pub mod parse;

/// Filesystem path utilities
pub mod path;

/// Time utilities
pub mod time;

// =======================
// TODO: Why is this here?
// =======================
use rand::{distributions::Alphanumeric, thread_rng, Rng};
pub fn gen_id(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}
// ======================
