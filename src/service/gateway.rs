use async_std::sync::Arc;
use std::convert::From;
use std::net::SocketAddr;

use super::reqrep::{PeerId, Publisher, RepProtocol, Reply, ReqProtocol, Request, Subscriber};
use crate::blockchain::{rocks::columns, RocksColumn, Slab, SlabStore};
use crate::{serial::deserialize, serial::serialize, Error, Result};
use async_executor::Executor;
use log::*;

pub type GatewaySlabsSubscriber = async_channel::Receiver<Slab>;

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
        rocks: RocksColumn<columns::Slabs>,
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
                let error = slabstore.put(deserialize(&slab)?)?;

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
    gateway_slabs_sub_s: async_channel::Sender<Slab>,
    gateway_slabs_sub_rv: GatewaySlabsSubscriber,
    is_running: bool,
}

impl GatewayClient {
    pub fn new(addr: SocketAddr, rocks: RocksColumn<columns::Slabs>) -> Result<Self> {
        let protocol = ReqProtocol::new(addr, String::from("GATEWAY CLIENT"));

        let slabstore = SlabStore::new(rocks)?;

        let (gateway_slabs_sub_s, gateway_slabs_sub_rv) = async_channel::unbounded::<Slab>();

        Ok(GatewayClient {
            protocol,
            slabstore,
            gateway_slabs_sub_s,
            gateway_slabs_sub_rv,
            is_running: false,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.protocol.start().await?;
        self.sync().await?;
        self.is_running = true;
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
                    break;
                }
            }
        }

        info!("End Syncing");
        Ok(last_index)
    }

    pub async fn get_slab(&mut self, index: u64) -> Result<Option<Slab>> {
        let handle_error = Arc::new(handle_error);
        let rep = self
            .protocol
            .request(
                GatewayCommand::GetSlab as u8,
                serialize(&index),
                handle_error,
            )
            .await?;

        if let Some(slab) = rep {
            let slab: Slab = deserialize(&slab)?;
            self.gateway_slabs_sub_s.send(slab.clone()).await?;
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

            let handle_error = Arc::new(handle_error);

            let rep = self
                .protocol
                .request(GatewayCommand::PutSlab as u8, slab.clone(), handle_error)
                .await?;

            if let Some(_) = rep {
                break;
            }
        }
        Ok(())
    }

    pub async fn get_last_index(&mut self) -> Result<u64> {
        let handle_error = Arc::new(handle_error);

        let rep = self
            .protocol
            .request(GatewayCommand::GetLastIndex as u8, vec![], handle_error)
            .await?;
        if let Some(index) = rep {
            return Ok(deserialize(&index)?);
        }
        Ok(0)
    }

    pub fn get_slabstore(&self) -> Arc<SlabStore> {
        self.slabstore.clone()
    }

    pub async fn start_subscriber(
        &self,
        sub_addr: SocketAddr,
        executor: Arc<Executor<'_>>,
    ) -> Result<GatewaySlabsSubscriber> {
        let mut subscriber = Subscriber::new(sub_addr, String::from("GATEWAY CLIENT"));
        subscriber.start().await?;
        executor
            .spawn(Self::subscribe_loop(
                subscriber,
                self.slabstore.clone(),
                self.gateway_slabs_sub_s.clone(),
            ))
            .detach();
        Ok(self.gateway_slabs_sub_rv.clone())
    }

    async fn subscribe_loop(
        mut subscriber: Subscriber,
        slabstore: Arc<SlabStore>,
        gateway_slabs_sub_s: async_channel::Sender<Slab>,
    ) -> Result<()> {
        loop {
            let slab = subscriber.fetch::<Slab>().await?;
            gateway_slabs_sub_s.send(slab.clone()).await?;
            slabstore.put(slab)?;
        }
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

fn handle_error(status_code: u32) {
    match status_code {
        1 => {
            warn!("Reply has an Error: Index is not updated");
        }
        2 => {
            warn!("Reply has an Error: Index Not Exist");
        }
        _ => {}
    }
}
