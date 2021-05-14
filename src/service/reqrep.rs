use std::io;

use crate::{Decodable, Encodable, Result};

use futures::FutureExt;
use rand::Rng;
use zeromq::*;

pub struct ReqRepAPI;

impl ReqRepAPI {
    pub async fn start() -> Result<()> {
        println!("start reqrep");

        let mut frontend = zeromq::RouterSocket::new();
        frontend.bind("tcp://127.0.0.1:3333").await?;

        let mut backend = zeromq::DealerSocket::new();
        backend.bind("tcp://127.0.0.1:4444").await?;
        loop {
            println!("start reqrep loop");
            futures::select! {
                frontend_mess = frontend.recv().fuse() => {
                    match frontend_mess {
                        Ok(message) => {
                            backend.send(message).await?;
                        }
                        Err(_) => {
                            // TODO
                        }
                    }
                },
                backend_mess = backend.recv().fuse() => {
                    match backend_mess {
                        Ok(message) => {
                            frontend.send(message).await?;
                        }
                        Err(_) => {
                            // TODO
                        }
                    }
                }
            };
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Request {
    command: u8,
    id: u32,
    payload: Vec<u8>,
}

impl Request {
    pub fn new(command: u8, payload: Vec<u8>) -> Request {
        let id = Self::gen_id();
        Request {
            command,
            id,
            payload,
        }
    }
    fn gen_id() -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen()
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }
}

#[derive(Debug, PartialEq)]
pub struct Reply {
    id: u32,
    error: u32,
    payload: Vec<u8>,
}

impl Reply {
    pub fn from(request: &Request, error: u32, payload: Vec<u8>) -> Reply {
        Reply {
            id: request.get_id(),
            error,
            payload,
        }
    }

    pub fn has_error(&self) -> bool {
        if self.error == 0 {
            false
        } else {
            true
        }
    }

    pub fn get_payload(&self) -> Vec<u8> {
        self.payload.clone()
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }
}

impl Encodable for Request {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.command.encode(&mut s)?;
        len += self.id.encode(&mut s)?;
        len += self.payload.encode(&mut s)?;
        Ok(len)
    }
}

impl Encodable for Reply {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.id.encode(&mut s)?;
        len += self.error.encode(&mut s)?;
        len += self.payload.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Request {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            command: Decodable::decode(&mut d)?,
            id: Decodable::decode(&mut d)?,
            payload: Decodable::decode(&mut d)?,
        })
    }
}

impl Decodable for Reply {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            id: Decodable::decode(&mut d)?,
            error: Decodable::decode(&mut d)?,
            payload: Decodable::decode(&mut d)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Reply, Request, Result};
    use crate::serial::{deserialize, serialize};

    #[test]
    fn serialize_and_deserialize_request_test() {
        let request = Request::new(2, vec![2, 3, 4, 6, 4]);
        let serialized_request = serialize(&request);
        assert!((deserialize(&serialized_request) as Result<bool>).is_err());
        let deserialized_request = deserialize(&serialized_request).ok();
        assert_eq!(deserialized_request, Some(request));
    }

    #[test]
    fn serialize_and_deserialize_reply_test() {
        let request = Request::new(2, vec![2, 3, 4, 6, 4]);
        let reply = Reply::from(&request, 0, vec![2, 3, 4, 6, 4]);
        let serialized_reply = serialize(&reply);
        assert!((deserialize(&serialized_reply) as Result<bool>).is_err());
        let deserialized_reply = deserialize(&serialized_reply).ok();
        assert_eq!(deserialized_reply, Some(reply));
    }
}
