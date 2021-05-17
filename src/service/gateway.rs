use async_std::sync::{Arc, Mutex};
use std::convert::TryInto;

use super::reqrep::{Publisher, RepProtocol, Reply, ReqProtocol, Request, Subscriber};
use crate::{Error, Result};

use async_executor::Executor;

pub type Slabs = Vec<Vec<u8>>;

pub struct GatewayService {
    slabs: Mutex<Slabs>,
    addr: String,
    publisher: Mutex<Publisher>,
}

impl GatewayService {
    pub fn new(addr: String, pub_addr: String) -> Arc<GatewayService> {
        let slabs = Mutex::new(vec![]);
        let publisher = Mutex::new(Publisher::new(pub_addr));
        Arc::new(GatewayService {
            slabs,
            addr,
            publisher,
        })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let mut socket = RepProtocol::new(self.addr.clone());

        let (send, recv) = socket.start().await?;
        println!("server started");

        self.publisher.lock().await.start().await?;

        println!("publisher started");

        let handle_request_task = executor.spawn(self.handle_request(send.clone(), recv.clone()));

        socket.run().await?;

        handle_request_task.cancel().await;
        Ok(())
    }

    async fn handle_request(
        self: Arc<Self>,
        send_queue: async_channel::Sender<Reply>,
        recv_queue: async_channel::Receiver<Request>,
    ) -> Result<()> {
        let data = vec![];

        loop {
            match recv_queue.recv().await {
                Ok(request) => {
                    match request.get_command() {
                        0 => {
                            // PUTSLAB
                            let slab = request.get_payload();
                            self.slabs.lock().await.push(slab.clone());

                            // publish to all subscribes
                            self.publisher.lock().await.publish(slab).await?;

                            println!("received putslab msg");
                        }
                        1 => {
                            // GETSLAB
                            println!("received getslab msg");
                        }
                        2 => {
                            // GETLASTINDEX
                            println!("received getlastindex msg");
                        }
                        _ => {
                            return Err(Error::ServicesError("wrong command"));
                        }
                    }
                    let rep = Reply::from(&request, 0, data.clone());
                    send_queue.send(rep.into()).await?;
                }
                Err(_) => {}
            }
        }
    }
}

pub struct GatewayClient {
    protocol: ReqProtocol,
}

impl GatewayClient {
    pub fn new(addr: String) -> GatewayClient {
        let protocol = ReqProtocol::new(addr);
        GatewayClient { protocol }
    }
    pub async fn start(&mut self) -> Result<()> {
        self.protocol.start().await?;
        Ok(())
    }

    pub async fn subscribe(&self, sub_addr: String) -> Result<Arc<Mutex<Subscriber>>> {
        let mut subscriber = Subscriber::new(sub_addr);
        subscriber.start().await?;
        Ok(Arc::new(Mutex::new(subscriber)))
    }

    pub async fn get_slab(&mut self, index: u32) -> Result<Vec<u8>> {
        self.protocol
            .request(GatewayCommand::GetSlab as u8, index.to_be_bytes().to_vec())
            .await
    }

    pub async fn put_slab(&mut self, data: Vec<u8>) -> Result<()> {
        self.protocol
            .request(GatewayCommand::PutSlab as u8, data.clone())
            .await?;
        Ok(())
    }
    pub async fn get_last_index(&mut self) -> Result<u32> {
        let rep = self
            .protocol
            .request(GatewayCommand::GetLastIndex as u8, vec![])
            .await?;
        let rep: [u8; 4] = rep.try_into().unwrap();
        Ok(u32::from_be_bytes(rep))
    }

    pub async fn fetch_slabs_loop(
        subscriber: Arc<Mutex<Subscriber>>,
        slabs: Arc<Mutex<Slabs>>,
    ) -> Result<()> {
        loop {
            let slab: Vec<u8>;
            {
                let mut subscriber = subscriber.lock().await;
                slab = subscriber.fetch().await?;
            }
            println!("received new slab from subscriber");
            slabs.lock().await.push(slab);
        }
    }
}

#[repr(u8)]
enum GatewayCommand {
    PutSlab,
    GetSlab,
    GetLastIndex,
}
