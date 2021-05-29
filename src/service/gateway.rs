use async_std::sync::Arc;
use std::convert::From;
use std::net::SocketAddr;
use std::path::Path;

use super::reqrep::{Publisher, RepProtocol, Reply, ReqProtocol, Request, Subscriber};
use crate::{
    serial::deserialize, serial::serialize, slab::Slab, slabstore::SlabStore, Error, Result,
};

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
    slabstore: Arc<SlabStore>,
    addr: SocketAddr,
    pub_addr: SocketAddr,
}

impl GatewayService {
    pub fn new(addr: SocketAddr, pub_addr: SocketAddr) -> Result<Arc<GatewayService>> {
        let slabstore = SlabStore::new(Path::new("slabstore.db"))?;

        Ok(Arc::new(GatewayService {
            slabstore,
            addr,
            pub_addr,
        }))
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let service_name = String::from("GATEWAY");

        let mut protocol = RepProtocol::new(service_name.clone(), self.addr.clone());

        let (send, recv) = protocol.start().await?;

        let (publish_queue, publish_recv_queue) = async_channel::unbounded::<Vec<u8>>();
        let publisher_task = executor.spawn(Self::start_publisher(
            self.pub_addr,
            service_name,
            publish_recv_queue.clone(),
        ));

        let handle_request_task =
            executor.spawn(self.handle_request(send.clone(), recv.clone(), publish_queue.clone()));

        protocol.run(executor.clone()).await?;

        let _ = publisher_task.cancel().await;
        let _ = handle_request_task.cancel().await;
        Ok(())
    }

    async fn start_publisher(
        pub_addr: SocketAddr,
        service_name: String,
        publish_recv_queue: async_channel::Receiver<Vec<u8>>,
    ) -> Result<()> {
        let mut publisher = Publisher::new(pub_addr, service_name);
        publisher.start(publish_recv_queue).await?;
        Ok(())
    }

    async fn handle_request(
        self: Arc<Self>,
        send_queue: async_channel::Sender<Reply>,
        recv_queue: async_channel::Receiver<Request>,
        publish_queue: async_channel::Sender<Vec<u8>>,
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
                            publish_queue.send(slab).await?;

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
    pub slabstore: Arc<SlabStore>,
}

impl GatewayClient {
    pub fn new(addr: SocketAddr, path: &str) -> Result<Self> {
        let protocol = ReqProtocol::new(addr);

        let slabstore = SlabStore::new(Path::new(path))?;

        Ok(GatewayClient {
            protocol,
            slabstore,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.protocol.start().await?;

        // start syncing
        let local_last_index = self.slabstore.get_last_index()?;
        let last_index = self.get_last_index().await?;

        if last_index > 0 {
            for index in (local_last_index + 1)..(last_index + 1) {
                self.get_slab(index).await?;
            }
        }

        Ok(())
    }

    pub async fn get_slab(&mut self, index: u64) -> Result<Vec<u8>> {
        let slab = self
            .protocol
            .request(GatewayCommand::GetSlab as u8, serialize(&index))
            .await?;
        self.slabstore.put(slab.clone())?;
        Ok(slab)
    }

    pub async fn put_slab(&mut self, mut slab: Slab) -> Result<()> {
        let last_index = self.get_last_index().await?;
        slab.set_index(last_index + 1);
        let slab = serialize(&slab);

        self.protocol
            .request(GatewayCommand::PutSlab as u8, slab.clone())
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

    pub fn get_slabstore(&self) -> Arc<SlabStore> {
        self.slabstore.clone()
    }

    pub async fn subscribe(slabstore: Arc<SlabStore>, sub_addr: SocketAddr) -> Result<()> {
        let mut subscriber = Subscriber::new(sub_addr);
        subscriber.start().await?;
        loop {
            let slab: Vec<u8>;
            slab = subscriber.fetch().await?;
            slabstore.put(slab)?;
        }
    }
}
