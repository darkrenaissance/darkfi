/// Seed connections session. Manages the creation of seed sessions. Used on
/// first time connecting to the network. The seed node stores a list of other
/// nodes in the network.
pub mod seed_session;

pub mod manual_session;

/// Inbound connections session. Manages the creation of inbound sessions. Used
/// to create an inbound session and start and stop the session.
///
/// Class consists of 3 pointers: a weak pointer to the peer-to-peer class, an
/// acceptor pointer, and a stoppable task pointer. Using a weak pointer to P2P
/// allows us to avoid circular dependencies.
pub mod inbound_session;

/// Outbound connections session. Manages the creation of outbound sessions.
/// Used to create an outbound session and stop and start the session.
///
/// Class consists of a weak pointer to the peer-to-peer interface and a vector
/// of outbound connection slots. Using a weak pointer to p2p allows us to avoid
/// circular dependencies. The vector of slots is wrapped in a mutex lock. This
/// is switched on everytime we instantiate a connection slot and insures that
/// no other part of the program uses the slots at the same time.
pub mod outbound_session;

/// Defines methods that are used across sessions. Implements registering the
/// channel and initializing the channel by performing a network handshake.
pub mod session;

pub use seed_session::SeedSession;
pub use manual_session::ManualSession;
pub use inbound_session::InboundSession;
pub use outbound_session::OutboundSession;
pub use session::Session;
