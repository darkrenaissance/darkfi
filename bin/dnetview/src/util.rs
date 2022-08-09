use crate::model::{ConnectInfo, Session};
use darkfi::{util::serial, Result};
use smol::Timer;
use std::time::Duration;

/// Sleep for any number of milliseconds.
pub async fn sleep(millis: u64) {
    Timer::after(Duration::from_millis(millis)).await;
}

pub fn make_node_id(node_name: &String) -> Result<String> {
    Ok(serial::serialize_hex(node_name))
}

pub fn make_session_id(node_id: &str, session: &Session) -> Result<String> {
    let mut num = 0_u64;

    let session_chars = match session {
        Session::Inbound => vec!['i', 'n'],
        Session::Outbound => vec!['o', 'u', 't'],
        Session::Manual => vec!['m', 'a', 'n'],
        Session::Offline => vec!['o', 'f', 'f'],
    };

    for i in session_chars {
        num += i as u64
    }

    for i in node_id.chars() {
        num += i as u64
    }

    Ok(serial::serialize_hex(&num))
}

pub fn make_connect_id(id: &u64) -> Result<String> {
    Ok(serial::serialize_hex(id))
}

pub fn make_empty_id(node_id: &str, session: &Session, count: u64) -> Result<String> {
    let count = count * 2;

    let mut num = 0_u64;

    let session_chars = match session {
        Session::Inbound => vec!['i', 'n'],
        Session::Outbound => vec!['o', 'u', 't'],
        Session::Manual => vec!['m', 'a', 'n'],
        Session::Offline => vec!['o', 'f', 'f'],
    };

    for i in session_chars {
        num += i as u64
    }

    for i in node_id.chars() {
        num += i as u64
    }

    num += count;

    Ok(serial::serialize_hex(&num))
}

pub fn is_empty_session(connects: &[ConnectInfo]) -> bool {
    return connects.iter().all(|conn| conn.is_empty)
}
