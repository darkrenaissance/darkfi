use async_std::sync::{Arc, Mutex};
use std::convert::TryInto;
use std::net::SocketAddr;

use super::reqrep::{Publisher, RepProtocol, Reply, ReqProtocol, Request, Subscriber};
use crate::{Error, Result};

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
    slabs: Mutex<Slabs>,
    addr: SocketAddr,
    publisher: Mutex<Publisher>,
}

impl GatewayService {
    pub fn new(addr: SocketAddr, pub_addr: SocketAddr) -> Arc<GatewayService> {
        let slabs = Mutex::new(vec![]);
        let publisher = Mutex::new(Publisher::new(pub_addr));
        Arc::new(GatewayService {
            slabs,
            addr,
            publisher,
        })
    }

    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {

        let mut protocol = RepProtocol::new(String::from("GATEWAY"),self.addr.clone());

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

                            info!("received putslab msg");
                        }
                        1 => {
                            // GETSLAB
                            info!("received getslab msg");
                        }
                        2 => {
                            // GETLASTINDEX
                            info!("received getlastindex msg");
                        }
                        _ => {
                            return Err(Error::ServicesError("received wrong command"));
                        }
                    }
                    let rep = Reply::from(&request, 0, data.clone());
                    send_queue.send(rep.into()).await?;
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
}

impl GatewayClient {
    pub fn new(addr: SocketAddr) -> GatewayClient {
        let protocol = ReqProtocol::new(addr);
        GatewayClient { protocol }
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
        let rep: [u8; 4] = rep.try_into().map_err(|_| crate::Error::TryIntoError)?;
        Ok(u32::from_be_bytes(rep))
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
