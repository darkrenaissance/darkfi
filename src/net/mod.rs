use smol::Async;
use std::net::TcpStream;

pub mod channel;
pub mod connector;
#[macro_use]
pub mod message_subscriber;
pub mod messages;
pub mod hosts;
pub mod p2p;
pub mod protocols;
pub mod proxy;
pub mod sessions;
pub mod settings;
pub mod utility;

pub type AsyncTcpStream = async_dup::Arc<Async<TcpStream>>;

pub use channel::{Channel, ChannelPtr};
pub use connector::Connector;
pub use message_subscriber::{MessageSubscription, MessageSubscriber};
pub use hosts::{HostsPtr, Hosts};
pub use p2p::P2p;
pub use proxy::Proxy;
pub use settings::{SettingsPtr, Settings};
