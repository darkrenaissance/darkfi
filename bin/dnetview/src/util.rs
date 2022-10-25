use darkfi::Result;

use crate::model::{ConnectInfo, Session};

pub fn make_node_id(node_name: &String) -> Result<String> {
    let mut id = hex::encode(node_name);
    id.insert_str(0, "NODE");
    Ok(id)
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

    let mut id = hex::encode(&num.to_ne_bytes());
    id.insert_str(0, "SESSION");
    Ok(id)
}

pub fn make_connect_id(id: &u64) -> Result<String> {
    let mut id = hex::encode(&id.to_ne_bytes());
    id.insert_str(0, "CONNECT");
    Ok(id)
}

pub fn make_empty_id(node_id: &str, session: &Session, count: u64) -> Result<String> {
    let count = count * 2;

    let mut num = 0_u64;

    let id = match session {
        Session::Inbound => {
            let session_chars = vec!['i', 'n'];
            for i in session_chars {
                num += i as u64
            }
            for i in node_id.chars() {
                num += i as u64
            }
            num += count;
            let mut id = hex::encode(&num.to_ne_bytes());
            id.insert_str(0, "EMPTYIN");
            id
        }
        Session::Outbound => {
            let session_chars = vec!['o', 'u', 't'];
            for i in session_chars {
                num += i as u64
            }
            for i in node_id.chars() {
                num += i as u64
            }
            num += count;
            let mut id = hex::encode(&num.to_ne_bytes());
            id.insert_str(0, "EMPTYOUT");
            id
        }
        Session::Manual => {
            let session_chars = vec!['m', 'a', 'n'];
            for i in session_chars {
                num += i as u64
            }
            for i in node_id.chars() {
                num += i as u64
            }
            num += count;
            let mut id = hex::encode(&num.to_ne_bytes());
            id.insert_str(0, "EMPTYMAN");
            id
        }
        Session::Offline => {
            let session_chars = vec!['o', 'f', 'f'];
            for i in session_chars {
                num += i as u64
            }
            for i in node_id.chars() {
                num += i as u64
            }
            num += count;
            let mut id = hex::encode(&num.to_ne_bytes());
            id.insert_str(0, "EMPTYOFF");
            id
        }
    };

    Ok(id)
}

pub fn is_empty_session(connects: &[ConnectInfo]) -> bool {
    return connects.iter().all(|conn| conn.is_empty)
}
