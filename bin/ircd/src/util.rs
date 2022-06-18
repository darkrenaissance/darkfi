use std::net::SocketAddr;

use log::warn;

use darkfi::{Error, Result};

pub fn clean_input(mut line: String, peer_addr: &SocketAddr) -> Result<String> {
    if line.is_empty() {
        warn!("Received empty line from {}. ", peer_addr);
        warn!("Closing connection.");
        return Err(Error::ChannelStopped)
    }

    if &line[(line.len() - 2)..] != "\r\n" {
        warn!("Closing connection.");
        return Err(Error::ChannelStopped)
    }

    // Remove CRLF
    line.pop();
    line.pop();

    Ok(line)
}
