use super::reqrep::{PeerId, RepProtocol, Reply, ReqProtocol, Request};

use super::btc::{BitcoinKeys, PubAddress};

use crate::{serial::deserialize, serial::serialize, Error, Result};
use crate::wallet::{CashierDb, CashierDbPtr};

use std::net::SocketAddr;
use async_std::sync::Arc;
use async_executor::Executor;
use log::*;

#[repr(u8)]
enum CashierError {
    NoError,
    UpdateIndex,
}

#[repr(u8)]
enum CashierCommand {
    GetDBTC,
    GetBTC,
}

pub struct CashierService {
    addr: SocketAddr,
    wallet: Arc<CashierDb>,
}

impl CashierService {
    pub fn new(
        addr: SocketAddr,
        wallet: Arc<CashierDb>,
    )-> Result<Arc<CashierService>> {

        Ok(Arc::new(CashierService {
            addr,
            wallet,
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
                    let cashier_wallet = self.wallet.clone();
                    let _ = executor
                        .spawn(Self::handle_request(
                            msg,
                            cashier_wallet,
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
        cashier_wallet: Arc<CashierDb>,
        send_queue: async_channel::Sender<(PeerId, Reply)>,
    ) -> Result<()> {
        let request = msg.1;
        let peer = msg.0;
        match request.get_command() {
            0 => {
                // Exchange zk_pubkey for bitcoin address
                let zkpub = request.get_payload();

                // Generate bitcoin Address
                let btc_keys = BitcoinKeys::new().unwrap();

                let btc_pub = btc_keys.get_pubkey();

                // add to cashier_wallet
                //

                let mut reply = Reply::from(&request, CashierError::NoError as u32, vec![]);

                reply.set_payload(btc_pub.to_bytes());

                // send reply
                send_queue.send((peer, reply)).await?;

                info!("Received dkey->btc msg");

            }
            1 => {
                // Withdraw
            }
            _ => {
                return Err(Error::ServicesError("received wrong command"));
            }
        }
        Ok(())
    }
}

pub struct CashierClient {
    protocol: ReqProtocol,
}

impl CashierClient {
    pub fn new(addr: SocketAddr) -> Result<Self> {
        let protocol = ReqProtocol::new(addr, String::from("CASHIER CLIENT"));

        Ok(CashierClient {
            protocol
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.protocol.start().await?;

        Ok(())
    }

    pub async fn get_address(&mut self, index: jubjub::SubgroupPoint) -> Result<Option<PubAddress>> {
        let handle_error = Arc::new(handle_error);
        let rep = self
            .protocol
            .request(
                CashierCommand::GetDBTC as u8,
                serialize(&index),
                handle_error,
            )
            .await?;

        if let Some(key) = rep {
            //let pubkey = BitcoinPubKey::from_slice(&key).unwrap();
            //let address: Address = Address::p2pkh(&pubkey, Network::Testnet);
            let address = BitcoinKeys::address_from_slice(&key).unwrap();
            return Ok(Some(address));
        }
        Ok(None)
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
