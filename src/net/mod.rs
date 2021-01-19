use smol::Async;
use std::net::TcpStream;

pub mod acceptor;
pub mod channel;
pub mod connector;
#[macro_use]
pub mod message_subscriber;
pub mod hosts;
pub mod messages;
pub mod p2p;
pub mod protocols;
pub mod proxy;
pub mod sessions;
pub mod settings;
pub mod utility;

pub type AsyncTcpStream = async_dup::Arc<Async<TcpStream>>;

pub use acceptor::{Acceptor, AcceptorPtr};
pub use channel::{Channel, ChannelPtr};
pub use connector::Connector;
pub use hosts::{Hosts, HostsPtr};
pub use message_subscriber::{MessageSubscriber, MessageSubscription};
pub use p2p::P2p;
pub use proxy::Proxy;
pub use settings::{Settings, SettingsPtr};
