use secp256k1::key::SecretKey;
use bitcoin::util::{ecdsa::PrivateKey, address::Address};
use super::reqrep::{PeerId, RepProtocol, Reply, ReqProtocol, Request};
use std::net::SocketAddr;
use async_std::sync::Arc;
use async_executor::Executor;

pub struct BitcoinAddr {
    secret_key: PrivateKey,
    pub_address: Address
}
impl BitcoinAddr {
    pub fn new(
    ) -> Result<Arc<BitcoinAddr>> {
        let secret_key = SecretKey::new();

    }
}

pub struct CashierService {
    addr: SocketAddr,
    pub_addr: SocketAddr,
}

impl CashierService {
    pub fn new(
        addr: SocketAddr,
        pub_addr: SocketAddr,
    )-> Result<Arc<CashierService>> {

        Ok(Arc::new(CashierService {
            addr,
            pub_addr,
        }))
    }
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        let service_name = String::from("CASHIER DAEMON");

        let mut protocol = RepProtocol::new(self.addr.clone(), service_name.clone());

        let (send, recv) = protocol.start().await?;

        let handle_request_task = executor.spawn(self.handle_request_loop(
            send.clone(),
            recv.clone(),
            executor.clone(),
        ));

        protocol.run(executor.clone()).await?;

        let _ = handle_request_task.cancel().await;
        Ok(())
    }

    async fn handle_request_loop(
        self: Arc<Self>,
        send_queue: async_channel::Sender<(PeerId, Reply)>,
        recv_queue: async_channel::Receiver<(PeerId, Request)>,
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
    ) -> Result<()> {
        let request = msg.1;
        let peer = msg.0;
        match request.get_command() {
            0 => {


            }
            1 => {

            }
            2 => {

            }
            _ => {
                return Err(Error::ServicesError("received wrong command"));
            }
        }
        Ok(())
    }
}
