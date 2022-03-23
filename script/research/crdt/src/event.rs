use std::{cmp::Ordering, io};

use darkfi::{
    net,
    util::serial::{serialize, Decodable, Encodable},
    Result,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub struct Event {
    // the msg in the event
    pub value: Vec<u8>,
    // the counter for lamport clock
    pub counter: u64,
    // It might be necessary to attach the node's name to the timestamp
    // so that it is possible to differentiate between events
    pub name: String,
}

impl Encodable for Event {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode(&mut s)?;
        len += self.counter.encode(&mut s)?;
        len += self.name.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Event {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            value: Decodable::decode(&mut d)?,
            counter: Decodable::decode(&mut d)?,
            name: Decodable::decode(&mut d)?,
        })
    }
}

impl Event {
    pub fn new<T: Encodable + Decodable>(value: &T, counter: u64, name: String) -> Self {
        Self { value: serialize(value), counter, name }
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
