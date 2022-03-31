use std::{cmp::Ordering, io};

use darkfi::{
    net,
    util::serial::{
        deserialize, serialize, Decodable, Encodable, SerialDecodable, SerialEncodable,
    },
    Result,
};

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub enum EventCommand {
    Sync = 0,
    Update = 1,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, SerialEncodable, SerialDecodable)]
pub struct Event {
    // the msg in the event
    pub value: Vec<u8>,
    // the counter for lamport clock
    pub counter: u64,
    // It might be necessary to attach the node's name to the timestamp
    // so that it is possible to differentiate between events
    pub name: String,
    pub command: EventCommand,
}

impl Encodable for EventCommand {
    fn encode<S: io::Write>(&self, s: S) -> darkfi::Result<usize> {
        let mut len = 0;
        match self {
            Self::Sync => {
                len += (0 as u8).encode(s)?;
            }
            Self::Update => {
                len += (1 as u8).encode(s)?;
            }
        }
        Ok(len)
    }
}

impl Decodable for EventCommand {
    fn decode<D: io::Read>(d: D) -> darkfi::Result<Self> {
        let com: u8 = Decodable::decode(d)?;
        Ok(match com {
            0 => Self::Sync,
            _ => Self::Update,
        })
    }
}

impl Event {
    pub fn new_update_event<T: Encodable>(value: T, counter: u64, name: String) -> Self {
        Self { value: serialize(&value), counter, name, command: EventCommand::Update }
    }

    pub fn new_sync_event(name: String) -> Self {
        Self { value: vec![], counter: 0, name, command: EventCommand::Sync }
    }

    pub fn get_value<T: Decodable>(&self) -> Result<T> {
        deserialize(&self.value)
    }
}

impl Ord for Event {
    fn cmp(&self, other: &Self) -> Ordering {
        let ord = self.counter.cmp(&other.counter);
        if ord == Ordering::Equal {
            return self.name.cmp(&other.name)
        }
        ord
    }
}

impl net::Message for Event {
    fn name() -> &'static str {
        "event"
    }
}
