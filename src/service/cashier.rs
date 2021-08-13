use super::reqrep::{PeerId, RepProtocol, Reply, ReqProtocol, Request};

use super::btc::{BitcoinKeys, PubAddress};

use super::GatewayClient;
use crate::blockchain::{ Slab };

use crate::{Error, Result};
use crate::serial::{Encodable, serialize, deserialize};
use crate::wallet::CashierDbPtr;
use crate::tx;
use crate::crypto::load_params;

use bellman::groth16;
use bls12_381::Bls12;

use async_executor::Executor;
use async_std::sync::Arc;
use log::*;
use std::net::SocketAddr;

#[repr(u8)]
enum CashierError {
    NoError,
    UpdateIndex,
}

#[repr(u8)]
enum CashierCommand {
    GetAddress,
    Withdraw,
}

pub struct CashierService {
    addr: SocketAddr,
    wallet: CashierDbPtr,
    gateway: GatewayClient,
    mint_params: groth16::Parameters<Bls12>,
    mint_pvk: groth16::PreparedVerifyingKey<Bls12>,
    spend_params: groth16::Parameters<Bls12>,
    spend_pvk: groth16::PreparedVerifyingKey<Bls12>,
}

impl CashierService {
    pub fn new(
        addr: SocketAddr,
        wallet: CashierDbPtr,
        gateway: GatewayClient,
    ) -> Result<Arc<CashierService>> {

        // Load trusted setup parameters
        let (mint_params, mint_pvk) = load_params("mint.params")?;
        let (spend_params, spend_pvk) = load_params("spend.params")?;

        Ok(Arc::new(CashierService {
            addr,
            wallet,
            gateway,
            mint_params,
            mint_pvk,
            spend_params,
            spend_pvk,
        }))
    }
    pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "Cashier", "Start Cashier");
        let service_name = String::from("CASHIER DAEMON");

        let mut protocol = RepProtocol::new(self.addr.clone(), service_name.clone());

        let (send, recv) = protocol.start().await?;

        let handle_request_task =
            executor.spawn(self.handle_request_loop(send.clone(), recv.clone(), executor.clone()));

        protocol.run(executor.clone()).await?;

        let _ = handle_request_task.cancel().await;

        Ok(())
    }

    async fn mint_dbtc(&mut self, dkey_pub: jubjub::SubgroupPoint, value: u64) -> Result<()> {
        // Change to adapter
        let cashier_secret = self.wallet.get_cashier_private().unwrap();

        let builder = tx::TransactionBuilder {
            clear_inputs: vec![tx::TransactionBuilderClearInputInfo {
                value: value,
                asset_id: 1,
                signature_secret: cashier_secret,
            }],
            inputs: vec![],
            outputs: vec![tx::TransactionBuilderOutputInfo {
                value: value,
                asset_id: 1,
                public: dkey_pub,
            }],
        };

        let mut tx_data = vec![];
        {
            // Build the tx
            let tx = builder.build(&self.mint_params, &self.spend_params);
            // Now serialize it
            tx.encode(&mut tx_data).expect("encode tx");
        }

        //Add to blockchain
        let slab = Slab::new(tx_data);
        //let mut gateway = self.gateway.lock().await;
        self.gateway.put_slab(slab).await.expect("put slab");

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
                            executor.clone()
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
        cashier_wallet: CashierDbPtr,
        send_queue: async_channel::Sender<(PeerId, Reply)>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        let request = msg.1;
        let peer = msg.0;
        match request.get_command() {
            0 => {
                debug!(target: "Cashier", "Get command");
                // Exchange zk_pubkey for bitcoin address
                let zkpub = request.get_payload();

                //check if key has already been issued
                let _check = cashier_wallet.get_keys_by_dkey(&zkpub);

                // Generate bitcoin Address
                let btc_keys = BitcoinKeys::new().unwrap();

                let btc_pub = btc_keys.get_pubkey();
                let btc_priv = btc_keys.get_privkey();

                // add pairings to db
                let _result = cashier_wallet.put_exchange_keys(zkpub, *btc_priv, *btc_pub);

                let mut reply = Reply::from(&request, CashierError::NoError as u32, vec![]);

                reply.set_payload(btc_pub.to_bytes());

                // send reply
                send_queue.send((peer, reply)).await?;
                info!("Received dkey->btc msg");

                // start scheduler for checking balance
                let _result = btc_keys.start_scheduler(executor.clone());
                info!("Waiting for address balance");

            }
            1 => {

                let _btc_address = request.get_payload();

                let address = cashier_wallet.get_cashier_public()?;

                let mut reply = Reply::from(&request, CashierError::NoError as u32, vec![]);

                reply.set_payload(serialize(&address));
                
                send_queue.send((peer, reply)).await?;

                info!("Received withdraw request");
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

        Ok(CashierClient { protocol })
    }

    pub async fn start(&mut self) -> Result<()> {
        debug!(target: "Cashier", "Start CashierClient");
        self.protocol.start().await?;

        Ok(())
    }

    pub async fn withdraw(
        &mut self,
        _btc_address: bitcoin::Address,
    ) -> Result<Option<jubjub::SubgroupPoint>> {
        let handle_error = Arc::new(handle_error);
        let rep = self
            .protocol
            .request(
                CashierCommand::Withdraw as u8,
                vec![],  //TODO convert btc_address to bytes,
                handle_error,
            )
            .await?;

        if let Some(key) = rep {
            let address = deserialize(&key)?;
            return Ok(Some(address));
        }
        Ok(None)
    }

    pub async fn get_address(
        &mut self,
        index: jubjub::SubgroupPoint,
    ) -> Result<Option<PubAddress>> {
        let handle_error = Arc::new(handle_error);
        let rep = self
            .protocol
            .request(
                CashierCommand::GetAddress as u8,
                serialize(&index),
                handle_error,
            )
            .await?;

        if let Some(key) = rep {
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
