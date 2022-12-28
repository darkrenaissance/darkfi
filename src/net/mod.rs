/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

/// Acceptor class handles the acceptance of inbound socket connections. It's
/// used to start listening on a local socket address, to accept incoming
/// connections and to handle network errors.
pub mod acceptor;

/// Async channel that handles the sending of messages across the network.
/// Public interface is used to create new channels, to stop and start
/// a channel, and to send messages.
///
/// Implements message functionality and the message subscriber subsystem.
pub mod channel;

/// Handles the creation of outbound connections. Used to establish an outbound
/// connection.
pub mod connector;

/// Hosts are a list of network addresses used when establishing an outbound
/// connection. Hosts are shared across the network through the address
/// protocol. When attempting to connect, a node will loop through addresses in
/// the host store until it finds ones to connect to.
pub mod hosts;

/// Generic publish/subscribe class that can dispatch any kind of message to a
/// subscribed list of dispatchers. Dispatchers subscribe to a single
/// message format of any type. This is a generalized version of the simple
/// publish-subscribe class in system::Subscriber.
///
/// Message Subsystem also enables the creation of new message subsystems,
/// adding new dispatchers and clearing inactive channels.
///
/// Message Subsystem maintains a list of dispatchers, which is a generalized
/// version of a subscriber. Pub-sub is called on dispatchers through the
/// functions 'subscribe' and 'notify'. Whereas system::Subscriber only allows
/// messages of a single type, dispatchers can handle any kind of message. This
/// generic message is called a payload and is processed and decoded by the
/// Message Dispatcher.
///
/// The Message Dispatcher is a class of subscribers that implements a
/// generic trait called Message Dispatcher Interface, which allows us to
/// process any kind of payload as a message.
pub mod message_subscriber;

/// Defines how to decode generic messages as well as implementing the common
/// network messages that are sent between nodes as described by the Protocol
/// submodule.
///
/// Implements a type called Packet which is the base message type. Packets are
/// converted into messages and passed to an event loop.
pub mod message;

/// P2P provides all core functionality to interact with the peer-to-peer
/// network.
///
/// Used to create a network, to start and run it, to broadcast messages across
/// all channels, and to manage the channel store.
///
/// The channel store is a hashmap of channel address that we can use to add and
/// remove channels or check whether a channel is already is in the store.
pub mod p2p;

/// Defines the networking protocol used at each stage in a connection. Consists
/// of a series of messages that are sent across the network at the different
/// connection stages.
///
/// When a node connects to a network for the first time, it must follow a seed
/// protocol, which provides it with a list of network hosts to connect to. To
/// establish a connection to another node, nodes must send version and version
/// acknowledgement messages. During a connection, nodes continually get address
/// and get-address messages to inform eachother about what nodes are on the
/// network. Nodes also send out a ping and pong message which keeps the network
/// from shutting down.
///
/// Protocol submodule also implements a jobs manager than handles the
/// asynchronous execution of the protocols.
pub mod protocol;

/// Defines the interaction between nodes during a connection. Consists of an
/// inbound session, which describes how to set up an incoming connection, and
/// an outbound session, which describes setting up an outbound connection. Also
/// describes the seed session, which is the type of connection used when a node
/// connects to the network for the first time. Implements the session trait
/// which describes the common functions across all sessions.
pub mod session;

/// Network configuration settings.
pub mod settings;

/// Network transport implementations.
pub mod transport;

/// Network constants for various validations.
pub mod constants;

pub use acceptor::{Acceptor, AcceptorPtr};
pub use channel::{Channel, ChannelPtr};
pub use connector::Connector;
pub use hosts::{Hosts, HostsPtr};
pub use message::Message;
pub use message_subscriber::MessageSubscription;
pub use p2p::{P2p, P2pPtr};
pub use protocol::{ProtocolBase, ProtocolBasePtr, ProtocolJobsManager, ProtocolJobsManagerPtr};
pub use session::{
    Session, SessionBitflag, SessionWeakPtr, SESSION_ALL, SESSION_INBOUND, SESSION_MANUAL,
    SESSION_OUTBOUND, SESSION_SEED,
};
pub use settings::{Settings, SettingsPtr};
