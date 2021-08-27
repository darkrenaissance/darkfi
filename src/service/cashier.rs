use super::btc::{BitcoinKeys, PubAddress};
use super::reqrep::{PeerId, RepProtocol, Reply, ReqProtocol, Request};
use super::GatewayClient;
use crate::blockchain::Slab;
use crate::crypto::load_params;
use crate::serial::{deserialize, serialize, Encodable};
use crate::tx;
use crate::wallet::CashierDbPtr;
use crate::{Error, Result};

use bellman::groth16;
use bls12_381::Bls12;
use ff::Field;
use rand::rngs::OsRng;

use async_executor::Executor;

use electrum_client::Client as ElectrumClient;
use log::*;

use async_std::sync::Arc;

use std::net::SocketAddr;

#[repr(u8)]
enum CashierError {
    NoError,
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
    btc_client: Arc<ElectrumClient>,
    mint_params: groth16::Parameters<Bls12>,
    mint_pvk: groth16::PreparedVerifyingKey<Bls12>,
    spend_params: groth16::Parameters<Bls12>,
    spend_pvk: groth16::PreparedVerifyingKey<Bls12>,
}

impl CashierService {
    pub fn new(
        addr: SocketAddr,
        btc_endpoint: String,
        wallet: CashierDbPtr,
        gateway: GatewayClient,
    ) -> Result<Arc<CashierService>> {
        // Load trusted setup parameters

        // Pull address from config later
        let client_address = btc_endpoint;

        // create client
        let btc_client = Arc::new(ElectrumClient::new(&client_address)?);

        let (mint_params, mint_pvk) = load_params("mint.params")?;
        let (spend_params, spend_pvk) = load_params("spend.params")?;

        Ok(Arc::new(CashierService {
            addr,
            wallet,
            gateway,
            btc_client,
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

        //let rpc_url: std::net::SocketAddr = config.rpc_url.parse()?;

        Ok(())
    }

    async fn mint_dbtc(&mut self, dkey_pub: jubjub::SubgroupPoint, value: u64) -> Result<()> {
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
                    let btc_client = self.btc_client.clone();
                    let _ = executor
                        .spawn(Self::handle_request(
                            msg,
                            btc_client,
                            cashier_wallet,
                            send_queue.clone(),
                            executor.clone(),
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
        btc_client: Arc<ElectrumClient>,
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
                let btc_keys = BitcoinKeys::new(btc_client).unwrap();

                let btc_pub = btc_keys.get_pubkey();
                let btc_priv = btc_keys.get_privkey();

                let _script = btc_keys.get_script();

                // add pairings to db
                let _result = cashier_wallet.put_exchange_keys(zkpub, *btc_priv, *btc_pub);

                let mut reply = Reply::from(&request, CashierError::NoError as u32, vec![]);

                reply.set_payload(btc_pub.to_bytes());

                // send reply
                send_queue.send((peer, reply)).await?;
                info!("Received dkey->btc msg");

                // start scheduler for checking balance
                debug!(target: "BTC", "Subscribing");

                let _ = btc_keys.start_subscribe(executor.clone()).await?;

                //self.mint_dbtc(deserialize(&zkpub).unwrap(), 100);

                info!("Waiting for address balance");
            }
            1 => {
                let btc_address = request.get_payload();
                //let btc_address: String = deserialize(&btc_address)?;
                //let btc_address = bitcoin::util::address::Address::from_str(&btc_address)?;
                //

                let cashier_public: jubjub::SubgroupPoint;

                if let Some(addr) = cashier_wallet.get_address_by_btc_key(&btc_address)? {
                    cashier_public = deserialize(&addr.1)?;
                } else {
                    let cashier_secret = jubjub::Fr::random(&mut OsRng);
                    cashier_public =
                        zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;
                    cashier_wallet.put_withdraw_keys(
                        btc_address,
                        serialize(&cashier_secret),
                        serialize(&cashier_public),
                    )?;
                }

                let mut reply = Reply::from(&request, CashierError::NoError as u32, vec![]);

                reply.set_payload(serialize(&cashier_public));

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

    pub async fn withdraw(&mut self, btc_address: String) -> Result<Option<jubjub::SubgroupPoint>> {
        let handle_error = Arc::new(handle_error);
        let rep = self
            .protocol
            .request(
                CashierCommand::Withdraw as u8,
                serialize(&btc_address),
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
        _ => {}
    }
}
