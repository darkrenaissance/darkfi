/// JSON-RPC primitives
pub mod jsonrpc;

/// Client-side JSON-RPC implementation
pub mod client;

/// Server-side JSON-RPC implementation
pub mod server;

#[cfg(feature = "websockets")]
/// Websockets client
pub mod websockets;
