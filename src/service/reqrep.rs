use std::{io, net::SocketAddr, sync::Arc};

use async_executor::Executor;
use async_std::prelude::*;
use bytes::Bytes;
use futures::FutureExt;
use log::*;
use rand::Rng;
use signal_hook::consts::SIGINT;
use signal_hook_async_std::Signals;
use zeromq::*;

use crate::{
    serial::{deserialize, serialize, Decodable, Encodable},
    Result,
};

pub type PeerId = Vec<u8>;

pub type Channels =
    (async_channel::Sender<(PeerId, Reply)>, async_channel::Receiver<(PeerId, Request)>);

enum NetEvent {
    Receive(zeromq::ZmqMessage),
    Send((PeerId, Reply)),
    Stop,
}

pub fn addr_to_string(addr: SocketAddr) -> String {
    format!("tcp://{}", addr.to_string())
}

pub struct RepProtocol {
    addr: SocketAddr,
    socket: zeromq::RouterSocket,
    recv_queue: async_channel::Receiver<(PeerId, Reply)>,
    send_queue: async_channel::Sender<(PeerId, Request)>,
    channels: Channels,
    service_name: String,
}

impl RepProtocol {
    pub fn new(addr: SocketAddr, service_name: String) -> RepProtocol {
        let socket = zeromq::RouterSocket::new();
        let (send_queue, recv_channel) = async_channel::unbounded::<(PeerId, Request)>();
        let (send_channel, recv_queue) = async_channel::unbounded::<(PeerId, Reply)>();

        let channels = (send_channel, recv_channel);

        RepProtocol { addr, socket, recv_queue, send_queue, channels, service_name }
    }

    pub async fn start(
        &mut self,
    ) -> Result<(async_channel::Sender<(PeerId, Reply)>, async_channel::Receiver<(PeerId, Request)>)>
    {
        let addr = addr_to_string(self.addr);
        self.socket.bind(addr.as_str()).await?;
        debug!(target: "REP PROTOCOL API", "{} SERVICE: Bound To {}", self.service_name, addr);
        Ok(self.channels.clone())
    }

    pub async fn run(&mut self, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "REP PROTOCOL API", "{} SERVICE: Running", self.service_name);

        let (stop_s, stop_r) = async_channel::unbounded::<()>();

        let signals = Signals::new(&[SIGINT])?;
        let handle = signals.handle();

        let signals_task = executor.spawn(async move {
            let mut signals = signals.fuse();
            while let Some(signal) = signals.next().await {
                match signal {
                    SIGINT => {
                        stop_s.send(()).await?;
                        break
                    }
                    _ => unreachable!(),
                }
            }
            Ok::<(), crate::Error>(())
        });

        loop {
            let event = futures::select! {
                msg = self.socket.recv().fuse() => NetEvent::Receive(msg?),
                msg = self.recv_queue.recv().fuse() => NetEvent::Send(msg?),
                _ = stop_r.recv().fuse() => NetEvent::Stop
            };

            match event {
                NetEvent::Receive(msg) => {
                    if let Some(peer) = msg.get(0) {
                        if let Some(request) = msg.get(1) {
                            let request: Vec<u8> = request.to_vec();
                            let request: Request = deserialize(&request)?;
                            self.send_queue.send((peer.to_vec(), request)).await?;
                        }
                    }
                }
                NetEvent::Send((peer, reply)) => {
                    let peer = Bytes::from(peer);
                    let mut msg: Vec<Bytes> = vec![peer];
                    let reply: Vec<u8> = serialize(&reply);
                    let reply = Bytes::from(reply);
                    msg.push(reply);

                    let reply = zeromq::ZmqMessage::try_from(msg)
                        .map_err(|_| crate::Error::TryFromError)?;

                    self.socket.send(reply).await?;
                }
                NetEvent::Stop => break,
            }
        }

        handle.close();
        signals_task.await?;

        debug!(target: "REP PROTOCOL API","{} SERVICE: Stopped", self.service_name);
        Ok(())
    }
}

pub struct ReqProtocol {
    addr: SocketAddr,
    socket: zeromq::DealerSocket,
    service_name: String,
}

impl ReqProtocol {
    pub fn new(addr: SocketAddr, service_name: String) -> ReqProtocol {
        let socket = zeromq::DealerSocket::new();
        ReqProtocol { addr, socket, service_name }
    }

    pub async fn start(&mut self) -> Result<()> {
        let addr = addr_to_string(self.addr);
        self.socket.connect(addr.as_str()).await?;
        debug!(target: "REQ PROTOCOL API","{} SERVICE: Connected To {}", self.service_name, self.addr);
        Ok(())
    }

    pub async fn request(
        &mut self,
        command: u8,
        data: Vec<u8>,
        handle_error: Arc<dyn Fn(u32) + Send + Sync>,
    ) -> Result<Option<Vec<u8>>> {
        let request = Request::new(command, data);
        let req = serialize(&request);
        let req = bytes::Bytes::from(req);
        let req: zeromq::ZmqMessage = req.into();

        self.socket.send(req).await?;
        debug!(
        target: "REQ PROTOCOL API",
                "{} SERVICE: Sent Request {{ command: {} }}",
                self.service_name, command
            );

        let rep: zeromq::ZmqMessage = self.socket.recv().await?;
        if let Some(reply) = rep.get(0) {
            let reply: Vec<u8> = reply.to_vec();

            let reply: Reply = deserialize(&reply)?;

            debug!(
            target: "REQ PROTOCOL API",
                    "{} SERVICE: Received Reply {{ error: {} }}",
                    self.service_name,
                    reply.has_error()
                );

            if reply.has_error() {
                handle_error(reply.get_error());
                return Ok(None)
            }

            if reply.get_id() != request.get_id() {
                warn!("Reply id is not equal to Request id");
                return Ok(None)
            }

            Ok(Some(reply.get_payload()))
        } else {
            Err(crate::Error::ZmqError("Couldn't parse ZmqMessage".to_string()))
        }
    }
}

pub struct Publisher {
    addr: SocketAddr,
    socket: zeromq::PubSocket,
    service_name: String,
}

impl Publisher {
    pub fn new(addr: SocketAddr, service_name: String) -> Publisher {
        let socket = zeromq::PubSocket::new();
        Publisher { addr, socket, service_name }
    }

    pub async fn start(&mut self, recv_queue: async_channel::Receiver<Vec<u8>>) -> Result<()> {
        let addr = addr_to_string(self.addr);
        self.socket.bind(addr.as_str()).await?;
        debug!(
            target: "PUBLISHER API",
            "{} SERVICE : Bound To {}",
            self.service_name, addr
        );
        loop {
            let msg = recv_queue.recv().await?;
            self.publish(msg).await?;
        }
    }

    async fn publish(&mut self, data: Vec<u8>) -> Result<()> {
        let data = Bytes::from(data);
        self.socket.send(data.into()).await?;
        Ok(())
    }
}

pub struct Subscriber {
    addr: SocketAddr,
    socket: zeromq::SubSocket,
    service_name: String,
}

impl Subscriber {
    pub fn new(addr: SocketAddr, service_name: String) -> Subscriber {
        let socket = zeromq::SubSocket::new();
        Subscriber { addr, socket, service_name }
    }

    pub async fn start(&mut self) -> Result<()> {
        let addr = addr_to_string(self.addr);
        self.socket.connect(addr.as_str()).await?;

        self.socket.subscribe("").await?;
        debug!(
            target: "SUBSCRIBER API",
            "{} SERVICE : Connected To {}",
            self.service_name, addr
        );
        Ok(())
    }

    pub async fn fetch<T: Decodable>(&mut self) -> Result<T> {
        let data = self.socket.recv().await?;
        match data.get(0) {
            Some(d) => {
                let data = d.to_vec();
                let data: T = deserialize(&data)?;
                Ok(data)
            }
            None => Err(crate::Error::ZmqError("Couldn't parse ZmqMessage".to_string())),
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
        Request { command, id, payload }
    }
    fn gen_id() -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen()
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn get_command(&self) -> u8 {
        self.command
    }

    pub fn get_payload(&self) -> Vec<u8> {
        self.payload.clone()
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
        Reply { id: request.get_id(), error, payload }
    }

    pub fn has_error(&self) -> bool {
        self.error != 0
    }

    pub fn get_error(&self) -> u32 {
        self.error
    }

    pub fn get_payload(&self) -> Vec<u8> {
        self.payload.clone()
    }

    pub fn set_payload(&mut self, payload: Vec<u8>) {
        self.payload = payload;
    }

    pub fn set_error(&mut self, error: u32) {
        self.error = error;
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
