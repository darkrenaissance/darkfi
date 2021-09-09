use super::btc::{BitcoinKeys, PubAddress};
use super::reqrep::{PeerId, RepProtocol, Reply, ReqProtocol, Request};
use crate::blockchain::Rocks;
use crate::client::Client;
use crate::serial::{deserialize, serialize};
use crate::wallet::{CashierDbPtr, WalletPtr};
use crate::{Error, Result};

use ff::Field;
use rand::rngs::OsRng;

use async_executor::Executor;
use electrum_client::Client as ElectrumClient;
use log::*;

use async_std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use std::path::PathBuf;

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
    btc_client: Arc<ElectrumClient>,
    client: Arc<Mutex<Client>>,
}

impl CashierService {
    pub async fn new(
        addr: SocketAddr,
        btc_endpoint: String,
        wallet: CashierDbPtr,
        client_wallet: WalletPtr,
        cashier_database_path: PathBuf,
        gateway_addrs: (SocketAddr, SocketAddr),
        params_paths: (PathBuf, PathBuf),
    ) -> Result<CashierService> {
        // Pull address from config later
        let client_address = btc_endpoint;

        // create btc client
        let btc_client = Arc::new(
            ElectrumClient::new(&client_address)
                .map_err(|err| crate::Error::from(super::BtcFailed::from(err)))?,
        );

        let rocks = Rocks::new(&cashier_database_path)?;

        let client = Client::new(rocks, gateway_addrs, params_paths, client_wallet.clone())?;

        let client = Arc::new(Mutex::new(client));

        Ok(CashierService {
            addr,
            wallet,
            btc_client,
            client,
        })
    }
    pub async fn start(&mut self, executor: Arc<Executor<'_>>) -> Result<()> {
        debug!(target: "CASHIER DAEMON", "Start Cashier");
        let service_name = String::from("CASHIER DAEMON");

        let mut protocol = RepProtocol::new(self.addr.clone(), service_name.clone());

        let (send, recv) = protocol.start().await?;

        self.wallet.init_db()?;

        let wallet = self.wallet.clone();
        let btc_client = self.btc_client.clone();

        let handle_request_task = executor.spawn(Self::handle_request_loop(
            send.clone(),
            recv.clone(),
            wallet.clone(),
            btc_client.clone(),
            executor.clone(),
        ));

        self.client.lock().await.start().await?;

        let (notify, recv_queue) = async_channel::unbounded::<(jubjub::SubgroupPoint, u64)>();

        let cashier_client_subscriber_task =
            executor.spawn(Client::connect_to_subscriber_from_cashier(
                self.client.clone(),
                executor.clone(),
                self.wallet.clone(),
                notify.clone(),
            ));

        let wallet = self.wallet.clone();
        let subscribe_to_withdraw_keys_task = executor.spawn(async move {
            loop {
                let (pub_key, amount) = recv_queue.recv().await.expect("Receive Own Coin");
                debug!(target: "CASHIER DAEMON", "Receive coin with following address and amount: {}, {}", pub_key, amount);
                let btc_addr = wallet.get_withdraw_coin_public_key_by_dkey_public(&pub_key, &serialize(&1)).expect("Get btc_key by pub_key");
                if let Some(addr) =  btc_addr {
                    // TODO send equivalent amount of btc to this address
                    // then delete this btc_addr from withdraw_keys records
                    wallet.delete_withdraw_key_record(&addr, &serialize(&1) ).expect("Delete withdraw key record");
                }

            }
        });

        protocol.run(executor.clone()).await?;

        let _ = handle_request_task.cancel().await;
        let _ = cashier_client_subscriber_task.cancel().await;
        let _ = subscribe_to_withdraw_keys_task.cancel().await;

        Ok(())
    }

    async fn _mint_dbtc(&mut self, dkey_pub: jubjub::SubgroupPoint, value: u64) -> Result<()> {
        self.client
            .lock()
            .await
            .send(dkey_pub, value, 1, true)
            .await?;
        Ok(())
    }

    async fn handle_request_loop(
        send_queue: async_channel::Sender<(PeerId, Reply)>,
        recv_queue: async_channel::Receiver<(PeerId, Request)>,
        wallet: CashierDbPtr,
        btc_client: Arc<ElectrumClient>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        loop {
            match recv_queue.recv().await {
                Ok(msg) => {
                    let _ = executor
                        .spawn(Self::handle_request(
                            msg,
                            btc_client.clone(),
                            wallet.clone(),
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
        btc_client: Arc<ElectrumClient>,
        cashier_wallet: CashierDbPtr,
        send_queue: async_channel::Sender<(PeerId, Reply)>,
    ) -> Result<()> {
        let request = msg.1;
        let peer = msg.0;
        debug!(target: "CASHIER DAEMON", "Get command");
        match request.get_command() {
            0 => {
                debug!(target: "CASHIER DAEMON", "Received deposit request");
                // Exchange zk_pubkey for bitcoin address
                let dpub = request.get_payload();

                //TODO: check if key has already been issued
                let dpub: jubjub::SubgroupPoint = deserialize(&dpub)?;
                let _check =
                    cashier_wallet.get_deposit_coin_keys_by_dkey_public(&dpub, &serialize(&1));

                // Generate bitcoin Address
                let btc_keys = BitcoinKeys::new(btc_client)?;

                let btc_pub = btc_keys.get_pubkey();
                let btc_priv = btc_keys.get_privkey();

                let _script = btc_keys.get_script();

                // add pairings to db
                let _result = cashier_wallet.put_exchange_keys(
                    &dpub,
                    &btc_priv.to_bytes(),
                    &btc_pub.to_bytes(),
                    &serialize(&1),
                );

                let mut reply = Reply::from(&request, CashierError::NoError as u32, vec![]);

                reply.set_payload(btc_pub.to_bytes());

                // send reply
                send_queue.send((peer, reply)).await?;

                // start scheduler for checking balance
                debug!(target: "CASHIER DAEMON", "Subscribing for deposit");

                let _ = btc_keys.start_subscribe().await?;

                //self.mint_dbtc(deserialize(&zkpub).unwrap(), 100);

                debug!(target: "CASHIER DAEMON","Waiting for address balance");
            }
            1 => {
                debug!(target: "CASHIER DAEMON", "Received withdraw request");
                let btc_address = request.get_payload();
                //let btc_address: String = deserialize(&btc_address)?;
                //let btc_address = bitcoin::util::address::Address::from_str(&btc_address)
                //   .map_err(|err| crate::Error::from(super::BtcFailed::from(err)))?;

                let cashier_public: jubjub::SubgroupPoint;

                if let Some(addr) = cashier_wallet
                    .get_withdraw_keys_by_coin_public_key(&btc_address, &serialize(&1))?
                {
                    cashier_public = addr.0;
                } else {
                    let cashier_secret = jubjub::Fr::random(&mut OsRng);
                    cashier_public =
                        zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;

                    cashier_wallet.put_withdraw_keys(
                        &btc_address,
                        &cashier_public,
                        &cashier_secret,
                        &serialize(&1),
                    )?;
                }

                let mut reply = Reply::from(&request, CashierError::NoError as u32, vec![]);

                reply.set_payload(serialize(&cashier_public));

                send_queue.send((peer, reply)).await?;
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
        debug!(target: "CASHIER CLIENT", "Start CashierClient");
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
            let address: jubjub::SubgroupPoint = deserialize(&key)?;
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
            let address = BitcoinKeys::address_from_slice(&key)?;
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
