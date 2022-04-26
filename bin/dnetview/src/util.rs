use crate::model::Session;
use darkfi::{util::serial, Result};
use rand::{thread_rng, Rng};

pub fn make_node_id(node_name: &String) -> Result<String> {
    Ok(serial::serialize_hex(node_name))
}

pub fn make_session_id(node_id: String, session: &Session) -> Result<String> {
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
    }

    for i in node_id.chars() {
        num += i as u64
    }

    Ok(serial::serialize_hex(&num))
}

pub fn make_connect_id(id: u64) -> Result<String> {
    Ok(serial::serialize_hex(&id))
}

// we use a random id for empty connections
pub fn generate_id() -> Result<String> {
    let mut rng = thread_rng();
    let id: u32 = rng.gen();
    Ok(serial::serialize_hex(&id))
}

pub fn make_empty_id(node_id: String, session: &Session, count: u64) -> Result<String> {
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
    }

    for i in node_id.chars() {
        num += i as u64
    }

    num += count;

    Ok(serial::serialize_hex(&num))
}
//pub fn is_empty_outbound(slots: Vec<Slot>) -> bool {
//    return slots.iter().all(|slot| slot.is_empty);
//}
