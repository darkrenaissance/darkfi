use crate::model::{ConnectInfo, Session};
use darkfi::{util::serial, Result};
use log::debug;

pub fn make_node_id(node_name: &String) -> Result<String> {
    //match serial::serialize_hex(node_name).as_str() {
    //    e => {
    //        debug!("NODE ID {} IS DERIVED FROM NODE NAME {} ", e, node_name);
    //    }
    //}
    Ok(serial::serialize_hex(node_name))
}

pub fn make_session_id(node_id: &String, session: &Session) -> Result<String> {
    let mut num = 0_u64;

    match session {
        Session::Inbound => {
            for i in ['i', 'n'] {
                num += i as u64;
            }
        }
        Session::Outbound => {
            for i in ['o', 'u', 't'] {
                num += i as u64;
            }
        }
        Session::Manual => {
            for i in ['m', 'a', 'n'] {
                num += i as u64;
            }
        }
        Session::Offline => {
            for i in ['o', 'f', 'f'] {
                num += i as u64
            }
        }
    }

    for i in node_id.chars() {
        num += i as u64
    }

    //match serial::serialize_hex(&num).as_str() {
    //    e => {
    //        debug!("SESSION ID {} IS DERIVED FROM NODE ID {} AND SESSION {:?}", e, node_id, session);
    //    }
    //}
    Ok(serial::serialize_hex(&num))
}

pub fn make_connect_id(id: &u64) -> Result<String> {
    //match serial::serialize_hex(id).as_str() {
    //    e => {
    //        debug!("CONNECT ID {} IS DERIVED FROM ID {}", e, id);
    //    }
    //}
    Ok(serial::serialize_hex(id))
}

pub fn make_empty_id(node_id: &String, session: &Session, count: u64) -> Result<String> {
    let count = count * 2;

    let mut num = 0_u64;

    match session {
        Session::Inbound => {
            for i in ['i', 'n'] {
                num += i as u64;
            }
        }
        Session::Outbound => {
            for i in ['o', 'u', 't'] {
                num += i as u64;
            }
        }
        Session::Manual => {
            for i in ['m', 'a', 'n'] {
                num += i as u64;
            }
        }
        Session::Offline => {
            for i in ['o', 'f', 'f'] {
                num += i as u64
            }
        }
    }

    for i in node_id.chars() {
        num += i as u64
    }

    num += count;

    //match serial::serialize_hex(&num).as_str() {
    //    e => {
    //        debug!("EMPTY ID {} IS DERIVED FROM NODE ID {} AND SESSION {:?} AND COUNT {}", e, node_id, session, count);
    //    }
    //}
    Ok(serial::serialize_hex(&num))
}

pub fn is_empty_session(connects: &Vec<ConnectInfo>) -> bool {
    return connects.iter().all(|conn| conn.is_empty)
}
