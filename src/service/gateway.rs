use std::convert::TryInto;

use super::reqrep::{Reply, Request};
use crate::serial::{deserialize, serialize};
use crate::{Error, Result};

use async_executor::Executor;
use async_std::sync::Arc;
use bytes::Bytes;
use futures::FutureExt;
use zeromq::*;

pub type Slabs = Vec<Vec<u8>>;

pub struct GatewayService {
    slabs: Slabs,
}

enum NetEvent {
    Receive(zeromq::ZmqMessage),
    Send(zeromq::ZmqMessage),
}

impl GatewayService {
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let mut worker = zeromq::RepSocket::new();
        worker.connect("tcp://127.0.0.1:4444").await?;

        let (send_queue_s, send_queue_r) = async_channel::unbounded::<zeromq::ZmqMessage>();

        let ex2 = executor.clone();
        loop {
            let event = futures::select! {
                request = worker.recv().fuse() => NetEvent::Receive(request?),
                reply = send_queue_r.recv().fuse() => NetEvent::Send(reply?)
            };

            let self2 = self.clone();
            match event {
                NetEvent::Receive(request) => {
                    let _ = ex2.spawn(self2.clone().handle_request(send_queue_s.clone(), request));
                }
                NetEvent::Send(reply) => {
                    worker.send(reply).await?;
                }
            }
        }
    }

    async fn handle_request(
        self: Arc<Self>,
        send_queue: async_channel::Sender<zeromq::ZmqMessage>,
        request: zeromq::ZmqMessage,
    ) -> Result<()> {
        let request: &Bytes = request.get(0).unwrap();
        let request: Vec<u8> = request.to_vec();
        let req: Request = deserialize(&request)?;

        let data = vec![];
        match req.get_command() {
            0 => {
                // PUTSLAB
                println!("receive PUTSLAB command");
            }
            1 => {
                // GETSLAB
                println!("receive GETSLAB command");
            }
            2 => {
                // GETLASTINDEX
                println!("receive GETLASTINDEX command");
            }
            _ => {
                return Err(Error::ServicesError("wrong command"));
            }
        }

        let rep = Reply::from(&req, 0, data);
        let rep: Vec<u8> = serialize(&rep);
        let rep = Bytes::from(rep);
        send_queue.send(rep.into()).await?;
        Ok(())
    }
}

struct GatewayClient {
    slabs: Slabs,
    sender: zeromq::ReqSocket,
}

impl GatewayClient {
    pub fn new() -> GatewayClient {
        let sender = zeromq::ReqSocket::new();
        GatewayClient {
            slabs: vec![],
            sender,
        }
    }
    pub async fn start(&mut self) -> Result<()> {
        self.sender.connect("tcp://127.0.0.1:3333").await?;
        Ok(())
    }
    async fn request(&mut self, command: GatewayCommand, data: Vec<u8>) -> Result<Vec<u8>> {
        let request = Request::new(command as u8, data);
        let req = serialize(&request);
        let req = bytes::Bytes::from(req);

        self.sender.send(req.into()).await?;

        let rep: zeromq::ZmqMessage = self.sender.recv().await?;
        let rep: &Bytes = rep.get(0).unwrap();
        let rep: Vec<u8> = rep.to_vec();

        let reply: Reply = deserialize(&rep)?;

        if reply.has_error() {
            return Err(crate::Error::ServicesError("response has an error"));
        }

        assert!(reply.get_id() == request.get_id());

        Ok(reply.get_payload())
    }

    pub async fn get_slab(&mut self, index: u32) -> Result<Vec<u8>> {
        self.request(GatewayCommand::GetSlab, index.to_be_bytes().to_vec())
            .await
    }

    pub async fn put_slab(&mut self, data: Vec<u8>) -> Result<()> {
        self.request(GatewayCommand::PutSlab, data).await?;
        Ok(())
    }
    pub async fn get_last_index(&mut self) -> Result<u32> {
        let rep = self.request(GatewayCommand::GetLastIndex, vec![]).await?;
        let rep: [u8; 4] = rep.try_into().unwrap();
        Ok(u32::from_be_bytes(rep))
    }
}

#[repr(u8)]
enum GatewayCommand {
    PutSlab,
    GetSlab,
    GetLastIndex,
}
