use super::reqrep::{Reply, Request};
use crate::serial::{deserialize, serialize};
use crate::Result;

use async_executor::Executor;
use async_std::sync::Arc;
use bytes::Bytes;
use futures::FutureExt;
use zeromq::*;

pub type Slabs = Vec<Vec<u8>>;

pub struct GatewayService;

enum NetEvent {
    RECEIVE(zeromq::ZmqMessage),
    SEND(zeromq::ZmqMessage),
}

impl GatewayService {
    pub async fn start(executor: Arc<Executor<'_>>) -> Result<()> {
        let mut worker = zeromq::RepSocket::new();
        worker.connect("tcp://127.0.0.1:4444").await?;

        let (send_queue_s, send_queue_r) = async_channel::unbounded::<zeromq::ZmqMessage>();

        let ex2 = executor.clone();
        loop {
            let event = futures::select! {
                request = worker.recv().fuse() => NetEvent::RECEIVE(request?),
                reply = send_queue_r.recv().fuse() => NetEvent::SEND(reply?)
            };

            match event {
                NetEvent::RECEIVE(request) => {
                    ex2.spawn(Self::handle_request(send_queue_s.clone(), request))
                        .detach();
                }
                NetEvent::SEND(reply) => {
                    worker.send(reply).await?;
                }
            }
        }
    }

    async fn handle_request(
        send_queue: async_channel::Sender<zeromq::ZmqMessage>,
        request: zeromq::ZmqMessage,
    ) -> Result<()> {
        let request: &Bytes = request.get(0).unwrap();
        let request: Vec<u8> = request.to_vec();
        let req: Request = deserialize(&request)?;

        // TODO
        // do things

        println!("Gateway service received a msg {:?}", req);

        let rep = Reply::from(&req, 0, "text".as_bytes().to_vec());
        let rep: Vec<u8> = serialize(&rep);
        let rep = Bytes::from(rep);
        send_queue.send(rep.into()).await?;
        Ok(())
    }
}

struct GatewayClient {
    slabs: Slabs,
}

impl GatewayClient {
    pub fn new() -> GatewayClient {
        GatewayClient { slabs: vec![] }
    }
    pub async fn start() {}

    pub async fn get_slab(index: u32) -> Vec<u8> {
        vec![]
    }

    pub async fn put_slab(&mut self, data: Vec<u8>) {
        self.slabs.push(data);
    }
}

#[repr(u8)]
enum GatewayCommand {
    PUTSLAB,
    GETSLAB,
    GETLASTINDEX,
}
