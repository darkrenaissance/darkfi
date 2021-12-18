pub mod aes;
pub mod channel;
pub mod cli_option;
pub mod control_message;
pub mod dbsql;
pub mod net;
pub mod slabs_manager;
pub mod utility;
pub mod error;

use async_std::sync::{Arc, Mutex};

pub type CiphertextHash = [u8; 32];
pub type MemPool = Arc<Mutex<Vec<(CiphertextHash, net::messages::SlabMessage)>>>;

