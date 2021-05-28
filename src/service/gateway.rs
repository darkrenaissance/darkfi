use async_std::sync::{Arc, Mutex};
use std::convert::From;
use std::net::SocketAddr;
use std::path::Path;

use super::reqrep::{Publisher, RepProtocol, Reply, ReqProtocol, Request, Subscriber};
use crate::{serial::deserialize, serial::serialize, slabstore::SlabStore, Error, Result};

use async_executor::Executor;
use log::*;

pub type Slabs = Vec<Vec<u8>>;

#[repr(u8)]
enum GatewayCommand {
    PutSlab,
    GetSlab,
    GetLastIndex,
}

pub struct GatewayService {
    slabstore: SlabStore,
    addr: SocketAddr,
    publisher: Mutex<Publisher>,
}

impl GatewayService {
    pub fn new(addr: SocketAddr, pub_addr: SocketAddr) -> Result<Arc<GatewayService>> {
        let publisher = Mutex::new(Publisher::new(pub_addr));

        let slabstore = SlabStore::new(Path::new("../slabstore.db"))?;

        Ok(Arc::new(GatewayService {
            slabstore,
            addr,
            publisher,
        }))
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let mut protocol = RepProtocol::new(String::from("GATEWAY"), self.addr.clone());

        let (send, recv) = protocol.start().await?;

        self.publisher.lock().await.start().await?;

        let handle_request_task = executor.spawn(self.handle_request(send.clone(), recv.clone()));

        protocol.run(executor.clone()).await?;

        let _ = handle_request_task.cancel().await;
        Ok(())
    }

    async fn handle_request(
        self: Arc<Self>,
        send_queue: async_channel::Sender<Reply>,
        recv_queue: async_channel::Receiver<Request>,
    ) -> Result<()> {
        loop {
            match recv_queue.recv().await {
                Ok(request) => {
                    // TODO spawn new task when receive new msg
                    match request.get_command() {
                        0 => {
                            // PUTSLAB

                            let slab = request.get_payload();

                            // add to slabstore
                            self.slabstore.put(slab.clone())?;

                            // send reply
                            let reply = Reply::from(&request, 0, vec![]);
                            send_queue.send(reply).await?;

                            // publish to all subscribes
                            self.publisher.lock().await.publish(slab).await?;

                            info!("received putslab msg");
                        }
                        1 => {
                            let index = request.get_payload();
                            let slab = self.slabstore.get(index)?;

                            let mut payload = vec![];

                            if let Some(sb) = slab {
                                payload = sb;
                            }

                            let reply = Reply::from(&request, 0, payload);
                            send_queue.send(reply).await?;

                            // GETSLAB
                            info!("received getslab msg");
                        }
                        2 => {
                            let index = self.slabstore.get_last_index_as_bytes()?;
                            let reply = Reply::from(&request, 0, index);
                            send_queue.send(reply).await?;

                            // GETLASTINDEX
                            info!("received getlastindex msg");
                        }
                        _ => {
                            return Err(Error::ServicesError("received wrong command"));
                        }
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }
        Ok(())
    }
}

pub struct GatewayClient {
    protocol: ReqProtocol,
    slabstore: SlabStore,
}

impl GatewayClient {
    pub fn new(addr: SocketAddr) -> Result<GatewayClient> {
        let protocol = ReqProtocol::new(addr);

        let slabstore = SlabStore::new(Path::new("slabstore.db"))?;

        Ok(GatewayClient {
            protocol,
            slabstore,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.protocol.start().await?;

        Ok(())
    }

    pub async fn subscribe(&self, sub_addr: SocketAddr) -> Result<Arc<Mutex<Subscriber>>> {
        let mut subscriber = Subscriber::new(sub_addr);
        subscriber.start().await?;
        Ok(Arc::new(Mutex::new(subscriber)))
    }

    pub async fn get_slab(&mut self, index: u64) -> Result<Vec<u8>> {
        self.protocol
            .request(GatewayCommand::GetSlab as u8, serialize(&index))
            .await
    }

    pub async fn put_slab(&mut self, data: Vec<u8>) -> Result<()> {
        self.protocol
            .request(GatewayCommand::PutSlab as u8, data.clone())
            .await?;
        Ok(())
    }
    pub async fn get_last_index(&mut self) -> Result<u64> {
        let rep = self
            .protocol
            .request(GatewayCommand::GetLastIndex as u8, vec![])
            .await?;
        Ok(deserialize(&rep)?)
    }
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
        info!("received new slab from subscriber");
        slabs.lock().await.push(slab);
    }
}
