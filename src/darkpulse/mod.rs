pub mod aes;
pub mod channel;
pub mod cli_option;
pub mod control_message;
pub mod dbsql;
pub mod net;
pub mod slabs_manager;
pub mod utility;

use async_std::sync::{Arc, Mutex};

pub type CiphertextHash = [u8; 32];
pub type MemPool = Arc<Mutex<Vec<(CiphertextHash, net::messages::SlabMessage)>>>;

pub use aes::{aes_decrypt, aes_encrypt, Ciphertext, Plaintext};
pub use channel::Channel;
pub use cli_option::CliOption;
pub use control_message::{ControlCommand, ControlMessage, MessagePayload};
pub use dbsql::Dbsql;
pub use net::{messages, messages::SlabMessage, protocol_slab::ProtocolSlab};
pub use slabs_manager::{SlabsManager, SlabsManagerSafe};
