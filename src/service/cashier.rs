use super::bridge;
use super::reqrep::{PeerId, RepProtocol, Reply, ReqProtocol, Request};
use crate::blockchain::Rocks;
use crate::client::Client;
use crate::serial::{deserialize, serialize};
use crate::wallet::{CashierDbPtr, WalletPtr};
use crate::{Error, Result};

use ff::Field;
use rand::rngs::OsRng;

use async_executor::Executor;
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
    client: Arc<Mutex<Client>>,
}

impl CashierService {
    pub async fn new(
        addr: SocketAddr,
        wallet: CashierDbPtr,
        client_wallet: WalletPtr,
        cashier_database_path: PathBuf,
        gateway_addrs: (SocketAddr, SocketAddr),
        params_paths: (PathBuf, PathBuf),
    ) -> Result<CashierService> {
        let rocks = Rocks::new(&cashier_database_path)?;

        let client = Client::new(rocks, gateway_addrs, params_paths, client_wallet.clone())?;

        let client = Arc::new(Mutex::new(client));

        Ok(CashierService {
            addr,
            wallet,
            client,
        })
    }
    pub async fn start(
        &mut self,
        executor: Arc<Executor<'_>>,
        // TODO: make this a vector of assets
        asset_id: jubjub::Fr,
    ) -> Result<()> {
        debug!(target: "CASHIER DAEMON", "Start Cashier");
        let service_name = String::from("CASHIER DAEMON");

        let mut protocol = RepProtocol::new(self.addr.clone(), service_name.clone());

        let (send, recv) = protocol.start().await?;

        self.wallet.init_db()?;

        let wallet = self.wallet.clone();

        let bridge = bridge::Bridge::new();

        cfg_if::cfg_if! {
            if #[cfg(feature = "default")]{
                // TODO: the endpoint should be generic according to asset_id
                let btc_endpoint: (bool, String) = 
                    (true, String::from("tcp://electrum.blockstream.info:60001"));

                let btc_client = super::btc::BtcClient::new(btc_endpoint)?;
                bridge.clone().add_clients(asset_id, Arc::new(btc_client)).await;
            }
        }

        let handle_request_task = executor.spawn(Self::handle_request_loop(
            send.clone(),
            recv.clone(),
            wallet.clone(),
            bridge.clone(),
            executor.clone(),
        ));

        self.client.lock().await.start().await?;

        let (notify, recv_coin) = async_channel::unbounded::<(jubjub::SubgroupPoint, u64)>();

        let cashier_client_subscriber_task =
            executor.spawn(Client::connect_to_subscriber_from_cashier(
                self.client.clone(),
                executor.clone(),
                self.wallet.clone(),
                notify.clone(),
            ));

        let wallet = self.wallet.clone();

        let ex = executor.clone();
        let subscribe_to_withdraw_keys_task = executor.spawn(async move {
            loop {
                let bridge = bridge.clone();
                let bridge_subscribtion  = bridge.subscribe(ex.clone()).await;
                let (pub_key, amount) = recv_coin.recv().await.expect("Receive Own Coin");
                debug!(target: "CASHIER DAEMON", "Receive coin with following address and amount: {}, {}", pub_key, amount);
                let coin_addr = wallet.get_withdraw_coin_public_key_by_dkey_public(&pub_key, &serialize(&1))
                    .expect("Get coin_key by pub_key");
                if let Some(addr) =  coin_addr {
                    // send equivalent amount of coin to this address
                    bridge_subscribtion.sender.send(
                        bridge::BridgeRequests {
                            asset_id,
                            payload: bridge::BridgeRequestsPayload::SendRequest(addr.clone(), amount)
                        }
                    ).await.expect("send request to bridge");

                    let res = bridge_subscribtion.receiver.recv().await.expect("bridge resonse");

                    if res.error == 0 {
                        match res.payload {
                            bridge::BridgeResponsePayload::SendResponse => {
                                // TODO Send the received coins to the main address
                                wallet.confirm_withdraw_key_record(&addr, &serialize(&1) )
                                    .expect("Confirm withdraw key record");
                            }
                            _ => {}
                        }

                    }


                }

            }
        });

        protocol.run(executor.clone()).await?;

        let _ = handle_request_task.cancel().await;
        let _ = cashier_client_subscriber_task.cancel().await;
        let _ = subscribe_to_withdraw_keys_task.cancel().await;

        Ok(())
    }

    async fn _mint_coin(
        &mut self,
        dkey_pub: jubjub::SubgroupPoint,
        value: u64,
        asset_id: jubjub::Fr,
    ) -> Result<()> {
        self.client
            .lock()
            .await
            .send(dkey_pub, value, asset_id, true)
            .await?;
        Ok(())
    }

    async fn handle_request_loop(
        send_queue: async_channel::Sender<(PeerId, Reply)>,
        recv_queue: async_channel::Receiver<(PeerId, Request)>,
        wallet: CashierDbPtr,
        bridge: Arc<bridge::Bridge>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        loop {
            match recv_queue.recv().await {
                Ok(msg) => {
                    let bridge = bridge.clone();
                    let bridge_subscribtion = bridge.subscribe(executor.clone()).await;
                    let _ = executor
                        .spawn(Self::handle_request(
                            msg,
                            bridge_subscribtion,
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
        bridge_subscribtion: bridge::BridgeSubscribtion,
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
                let (asset_id, dpub): (jubjub::Fr, jubjub::SubgroupPoint) =
                    deserialize(&request.get_payload())?;

                let _check =
                    cashier_wallet.get_deposit_coin_keys_by_dkey_public(&dpub, &serialize(&1));

                bridge_subscribtion
                    .sender
                    .send(bridge::BridgeRequests {
                        asset_id,
                        payload: bridge::BridgeRequestsPayload::WatchRequest,
                    })
                    .await?;

                let bridge_res = bridge_subscribtion.receiver.recv().await?;

                match bridge_res.payload {
                    bridge::BridgeResponsePayload::WatchResponse(coin_priv, coin_pub) => {
                        // add pairings to db
                        let _result = cashier_wallet.put_exchange_keys(
                            &dpub,
                            &coin_priv,
                            &coin_pub,
                            &serialize(&asset_id),
                        );

                        let mut reply = Reply::from(&request, CashierError::NoError as u32, vec![]);

                        reply.set_payload(coin_pub);

                        // send reply
                        send_queue.send((peer, reply)).await?;
                    }
                    _ => {}
                }

                debug!(target: "CASHIER DAEMON","Waiting for address balance");
            }
            1 => {
                debug!(target: "CASHIER DAEMON", "Received withdraw request");
                let (asset_id, coin_address): (jubjub::Fr, Vec<u8>) =
                    deserialize(&request.get_payload())?;

                let asset_id = serialize(&asset_id);

                let cashier_public: jubjub::SubgroupPoint;

                if let Some(addr) =
                    cashier_wallet.get_withdraw_keys_by_coin_public_key(&coin_address, &asset_id)?
                {
                    cashier_public = addr.public;
                } else {
                    let cashier_secret = jubjub::Fr::random(&mut OsRng);
                    cashier_public =
                        zcash_primitives::constants::SPENDING_KEY_GENERATOR * cashier_secret;

                    cashier_wallet.put_withdraw_keys(
                        &coin_address,
                        &cashier_public,
                        &cashier_secret,
                        &asset_id,
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

    pub async fn withdraw(
        &mut self,
        asset_id: jubjub::Fr,
        coin_address: Vec<u8>,
    ) -> Result<Option<jubjub::SubgroupPoint>> {
        let handle_error = Arc::new(handle_error);
        let rep = self
            .protocol
            .request(
                CashierCommand::Withdraw as u8,
                serialize(&(asset_id, coin_address)),
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
        asset_id: jubjub::Fr,
        index: jubjub::SubgroupPoint,
    ) -> Result<Option<Vec<u8>>> {
        let handle_error = Arc::new(handle_error);
        let rep = self
            .protocol
            .request(
                CashierCommand::GetAddress as u8,
                serialize(&(asset_id, index)),
                handle_error,
            )
            .await?;

        if let Some(key) = rep {
            return Ok(Some(key));
        }
        Ok(None)
    }
}

fn handle_error(status_code: u32) {
    match status_code {
        _ => {}
    }
}
