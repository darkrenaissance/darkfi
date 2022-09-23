#[cfg(feature = "async-runtime")]
pub mod async_serial;
#[cfg(feature = "async-runtime")]
pub mod async_util;

pub mod cli;
pub mod clock;
pub mod endian;
pub mod file;
pub mod net_name;
pub mod parse;
pub mod path;
pub mod serial;
pub mod time;

#[cfg(feature = "async-runtime")]
pub use async_util::sleep;

pub use net_name::NetworkName;
pub use parse::{decode_base10, encode_base10};
pub use path::{expand_path, join_config_path, load_keypair_to_str};

pub use clock::{Clock, Ticks};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
pub use time::{check_clock, ntp_request, unix_timestamp, NanoTimestamp, Timestamp};
pub fn gen_id(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}
