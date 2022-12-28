/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use std::collections::HashMap;

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use futures::stream::{FuturesUnordered, StreamExt};
use log::{debug, error};

use darkfi::{
    crypto::{keypair::PublicKey, types::*},
    util::NetworkName,
    wallet::cashierdb::TokenKey,
    Error, Result,
};

pub struct BridgeRequests {
    pub network: NetworkName,
    pub payload: BridgeRequestsPayload,
}

pub struct BridgeResponse {
    pub error: BridgeResponseError,
    pub payload: BridgeResponsePayload,
}

pub enum BridgeRequestsPayload {
    Send(Vec<u8>, u64),      // send (address, amount)
    Watch(Option<TokenKey>), // if already has a keypair
}

pub enum BridgeResponsePayload {
    Watch(TokenSubscribtion),
    Address(String),
    Send,
    Empty,
}

#[repr(u8)]
pub enum BridgeResponseError {
    NoError,
    NotSupportedClient,
    BridgeWatchSubscribtionError,
    BridgeSendSubscribtionError,
}

pub struct BridgeSubscribtion {
    pub sender: async_channel::Sender<BridgeRequests>,
    pub receiver: async_channel::Receiver<BridgeResponse>,
}

#[derive(Debug)]
pub struct TokenSubscribtion {
    pub private_key: Vec<u8>,
    pub public_key: String,
}

#[derive(Debug)]
pub struct TokenNotification {
    pub network: NetworkName,
    pub token_id: DrkTokenId,
    pub drk_pub_key: PublicKey,
    pub received_balance: u64,
    pub decimals: u16,
}

pub struct Bridge {
    clients: Mutex<HashMap<NetworkName, Arc<dyn NetworkClient + Send + Sync>>>,
    notifiers: FuturesUnordered<async_channel::Receiver<TokenNotification>>,
}

impl Bridge {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { clients: Mutex::new(HashMap::new()), notifiers: FuturesUnordered::new() })
    }

    pub async fn add_clients(
        self: Arc<Self>,
        network: NetworkName,
        client: Arc<dyn NetworkClient + Send + Sync>,
    ) -> Result<()> {
        debug!(target: "BRIDGE", "Adding new client");

        let client2 = client.clone();
        let notifier = client2.get_notifier().await?;

        if !notifier.is_closed() {
            self.notifiers.push(notifier);
        }

        self.clients.lock().await.insert(network, client.clone());

        Ok(())
    }

    pub async fn listen(self: Arc<Self>) -> Option<Result<TokenNotification>> {
        if !self.notifiers.is_empty() {
            debug!(target: "BRIDGE", "Start listening for new notifications");
            let notification = self
                .notifiers
                .iter()
                .map(|n| n.recv())
                .collect::<FuturesUnordered<async_channel::Recv<TokenNotification>>>()
                .next()
                .await
                .map(|o| o.map_err(Error::from));

            debug!(target: "BRIDGE", "Stop listening for new notifications");

            notification
        } else {
            None
        }
    }

    pub async fn subscribe(
        self: Arc<Self>,
        drk_pub_key: PublicKey,
        mint: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> BridgeSubscribtion {
        debug!(target: "BRIDGE", "Start new subscription");
        let (sender, req) = async_channel::unbounded();
        let (rep, receiver) = async_channel::unbounded();

        executor
            .spawn(self.listen_for_new_subscription(req, rep, drk_pub_key, mint, executor.clone()))
            .detach();

        BridgeSubscribtion { sender, receiver }
    }

    async fn listen_for_new_subscription(
        self: Arc<Self>,
        req: async_channel::Receiver<BridgeRequests>,
        rep: async_channel::Sender<BridgeResponse>,
        drk_pub_key: PublicKey,
        mint: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<()> {
        debug!(target: "BRIDGE", "Listen for new subscriptions");
        let req = req.recv().await?;

        let network = req.network;

        if !self.clients.lock().await.contains_key(&network) {
            let res = BridgeResponse {
                error: BridgeResponseError::NotSupportedClient,
                payload: BridgeResponsePayload::Empty,
            };
            rep.send(res).await?;
            return Ok(())
        }

        let mut mint_address: Option<String> = mint.clone();

        if mint.is_some() && mint.unwrap().is_empty() {
            mint_address = None;
        }

        let client: Arc<dyn NetworkClient + Send + Sync>;
        // avoid deadlock
        {
            let c = &self.clients.lock().await[&network];
            client = c.clone();
        }

        let res: BridgeResponse;

        match req.payload {
            BridgeRequestsPayload::Watch(val) => match val {
                Some(token_key) => {
                    let pub_key = client
                        .subscribe_with_keypair(
                            token_key.secret_key,
                            token_key.public_key,
                            drk_pub_key,
                            mint_address,
                            executor,
                        )
                        .await;

                    if pub_key.is_err() {
                        error!(target: "BRIDGE", "{}", pub_key.unwrap_err().to_string());
                        res = BridgeResponse {
                            error: BridgeResponseError::BridgeWatchSubscribtionError,
                            payload: BridgeResponsePayload::Empty,
                        };
                    } else {
                        res = BridgeResponse {
                            error: BridgeResponseError::NoError,
                            payload: BridgeResponsePayload::Address(pub_key?),
                        };
                    }
                }
                None => {
                    let sub = client.subscribe(drk_pub_key, mint_address, executor).await;
                    if sub.is_err() {
                        error!(target: "BRIDGE", "{}", sub.unwrap_err().to_string());
                        res = BridgeResponse {
                            error: BridgeResponseError::BridgeWatchSubscribtionError,
                            payload: BridgeResponsePayload::Empty,
                        };
                    } else {
                        let sub = sub?;
                        res = BridgeResponse {
                            error: BridgeResponseError::NoError,
                            payload: BridgeResponsePayload::Watch(sub),
                        };
                    }
                }
            },
            BridgeRequestsPayload::Send(addr, amount) => {
                let result = client.send(addr, mint_address, amount).await;

                if result.is_err() {
                    error!(target: "BRIDGE", "{}", result.unwrap_err().to_string());
                    res = BridgeResponse {
                        error: BridgeResponseError::BridgeSendSubscribtionError,
                        payload: BridgeResponsePayload::Empty,
                    };
                } else {
                    res = BridgeResponse {
                        error: BridgeResponseError::NoError,
                        payload: BridgeResponsePayload::Send,
                    };
                }
            }
        }

        rep.send(res).await?;

        Ok(())
    }
}

#[async_trait]
pub trait NetworkClient {
    async fn subscribe(
        self: Arc<Self>,
        drk_pub_key: PublicKey,
        mint: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<TokenSubscribtion>;

    // should check if the keypair in not already subscribed
    async fn subscribe_with_keypair(
        self: Arc<Self>,
        private_key: Vec<u8>,
        public_key: Vec<u8>,
        drk_pub_key: PublicKey,
        mint: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<String>;

    async fn get_notifier(self: Arc<Self>) -> Result<async_channel::Receiver<TokenNotification>>;

    async fn send(
        self: Arc<Self>,
        address: Vec<u8>,
        mint: Option<String>,
        amount: u64,
    ) -> Result<()>;
}
