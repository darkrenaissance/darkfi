use async_std::sync::Arc;
use std::convert::From;
use std::net::SocketAddr;

use super::reqrep::{PeerId, Publisher, RepProtocol, Reply, ReqProtocol, Request, Subscriber};
use crate::{
    serial::deserialize, serial::serialize, slab::Slab, slabstore::SlabStore, Error, Result, rocks::Rocks
};

use async_executor::Executor;
use log::*;

pub type Slabs = Vec<Vec<u8>>;

#[repr(u8)]
enum GatewayError {
    NoError,
    UpdateIndex,
    IndexNotExist,
}

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
    pub fn new(
        addr: SocketAddr,
        pub_addr: SocketAddr,
        rocks: Rocks,
    ) -> Result<Arc<GatewayService>> {
        let slabstore = SlabStore::new(rocks)?;

        Ok(Arc::new(GatewayService {
            slabstore,
            addr,
            pub_addr,
        }))
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let service_name = String::from("GATEWAY DAEMON");

        let mut protocol = RepProtocol::new(self.addr.clone(), service_name.clone());

        let (send, recv) = protocol.start().await?;

        let (publish_queue, publish_recv_queue) = async_channel::unbounded::<Vec<u8>>();
        let publisher_task = executor.spawn(Self::start_publisher(
            self.pub_addr,
            service_name,
            publish_recv_queue.clone(),
        ));

        let handle_request_task = executor.spawn(self.handle_request_loop(
            send.clone(),
            recv.clone(),
            publish_queue.clone(),
            executor.clone(),
        ));

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

    async fn handle_request_loop(
        self: Arc<Self>,
        send_queue: async_channel::Sender<(PeerId, Reply)>,
        recv_queue: async_channel::Receiver<(PeerId, Request)>,
        publish_queue: async_channel::Sender<Vec<u8>>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        loop {
            match recv_queue.recv().await {
                Ok(msg) => {
                    let slabstore = self.slabstore.clone();
                    let _ = executor
                        .spawn(Self::handle_request(
                            msg,
                            slabstore,
                            send_queue.clone(),
                            publish_queue.clone(),
                        ))
                        .detach();
                }
                Err(_) => {
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_request(
        msg: (PeerId, Request),
        slabstore: Arc<SlabStore>,
        send_queue: async_channel::Sender<(PeerId, Reply)>,
        publish_queue: async_channel::Sender<Vec<u8>>,
    ) -> Result<()> {
        let request = msg.1;
        let peer = msg.0;
        match request.get_command() {
            0 => {
                // PUTSLAB

                let slab = request.get_payload();

                // add to slabstore
                let error = slabstore.put(slab.clone())?;

                let mut reply = Reply::from(&request, GatewayError::NoError as u32, vec![]);

                if let None = error {
                    reply.set_error(GatewayError::UpdateIndex as u32);
                }

                // send reply
                send_queue.send((peer, reply)).await?;

                // publish to all subscribes
                publish_queue.send(slab).await?;

                info!("Received putslab msg");
            }
            1 => {
                let index = request.get_payload();
                let slab = slabstore.get(index)?;

                let mut reply = Reply::from(&request, GatewayError::NoError as u32, vec![]);

                if let Some(payload) = slab {
                    reply.set_payload(payload);
                } else {
                    reply.set_error(GatewayError::IndexNotExist as u32);
                }

                send_queue.send((peer, reply)).await?;

                // GETSLAB
                info!("Received getslab msg");
            }
            2 => {
                let index = slabstore.get_last_index_as_bytes()?;

                let reply = Reply::from(&request, GatewayError::NoError as u32, index);
                send_queue.send((peer, reply)).await?;

                // GETLASTINDEX
                info!("Received getlastindex msg");
            }
            _ => {
                return Err(Error::ServicesError("received wrong command"));
            }
        }
        Ok(())
    }
}

pub struct GatewayClient {
    protocol: ReqProtocol,
    slabstore: Arc<SlabStore>,
}

impl GatewayClient {
    pub fn new(addr: SocketAddr, rocks: Rocks) -> Result<Self> {
        let protocol = ReqProtocol::new(addr, String::from("GATEWAY CLIENT"));

        let slabstore = SlabStore::new(rocks)?;

        Ok(GatewayClient {
            protocol,
            slabstore,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.protocol.start().await?;
        self.sync().await?;

        Ok(())
    }

    pub async fn sync(&mut self) -> Result<u64> {
        info!("Start Syncing");
        let local_last_index = self.slabstore.get_last_index()?;

        let last_index = self.get_last_index().await?;

        assert!(last_index >= local_last_index);

        if last_index > 0 {
            for index in (local_last_index + 1)..(last_index + 1) {
                if let None = self.get_slab(index).await? {
                    warn!("Index not exist");
                    break;
                }
            }
        }

        info!("End Syncing");
        Ok(last_index)
    }

    pub async fn get_slab(&mut self, index: u64) -> Result<Option<Vec<u8>>> {
        let rep = self
            .protocol
            .request(GatewayCommand::GetSlab as u8, serialize(&index))
            .await?;

        if let Some(slab) = rep {
            self.slabstore.put(slab.clone())?;
            return Ok(Some(slab));
        }
        Ok(None)
    }

    pub async fn put_slab(&mut self, mut slab: Slab) -> Result<()> {
        loop {
            let last_index = self.sync().await?;
            slab.set_index(last_index + 1);
            let slab = serialize(&slab);

            let rep = self
                .protocol
                .request(GatewayCommand::PutSlab as u8, slab.clone())
                .await?;

            if let Some(_) = rep {
                break;
            }
        }
        Ok(())
    }

    pub async fn get_last_index(&mut self) -> Result<u64> {
        let rep = self
            .protocol
            .request(GatewayCommand::GetLastIndex as u8, vec![])
            .await?;
        if let Some(index) = rep {
            return Ok(deserialize(&index)?);
        }
        Ok(0)
    }

    pub fn get_slabstore(&self) -> Arc<SlabStore> {
        self.slabstore.clone()
    }

    pub async fn start_subscriber(sub_addr: SocketAddr) -> Result<Subscriber> {
        let mut subscriber = Subscriber::new(sub_addr, String::from("GATEWAY CLIENT"));
        subscriber.start().await?;
        Ok(subscriber)
    }

    pub async fn subscribe(mut subscriber: Subscriber, slabstore: Arc<SlabStore>) -> Result<()> {
        loop {
            let slab: Vec<u8>;
            slab = subscriber.fetch().await?;
            slabstore.put(slab)?;
        }
    }
}
