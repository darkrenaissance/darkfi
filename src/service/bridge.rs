use crate::util::NetworkName;
use crate::{Error, Result};

use async_trait::async_trait;
use futures::stream::FuturesUnordered;
use futures::stream::StreamExt;
use log::*;

use async_std::sync::{Arc, Mutex};
use std::collections::HashMap;

pub struct BridgeRequests {
    pub network: NetworkName,
    pub payload: BridgeRequestsPayload,
}

pub struct BridgeResponse {
    pub error: BridgeResponseError,
    pub payload: BridgeResponsePayload,
}

pub enum BridgeRequestsPayload {
    Send(Vec<u8>, u64),                // send (address, amount)
    Watch(Option<(Vec<u8>, Vec<u8>)>), // if already has a keypair
}

pub enum BridgeResponsePayload {
    Watch(Vec<u8>, String),
    Address(String),
    Send,
    Empty,
}

#[repr(u8)]
pub enum BridgeResponseError {
    NoError,
    NotSupportedClient,
}

pub struct BridgeSubscribtion {
    pub sender: async_channel::Sender<BridgeRequests>,
    pub receiver: async_channel::Receiver<BridgeResponse>,
}

pub struct TokenSubscribtion {
    pub secret_key: Vec<u8>,
    pub public_key: String,
}

#[derive(Debug)]
pub struct TokenNotification {
    pub network: NetworkName,
    pub token_id: jubjub::Fr,
    pub drk_pub_key: jubjub::SubgroupPoint,
    pub received_balance: u64,
}

pub struct Bridge {
    clients: Mutex<HashMap<NetworkName, Arc<dyn NetworkClient + Send + Sync>>>,
    notifiers: FuturesUnordered<async_channel::Receiver<TokenNotification>>,
}

impl Bridge {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            clients: Mutex::new(HashMap::new()),
            notifiers: FuturesUnordered::new(),
        })
    }

    pub async fn add_clients(
        self: Arc<Self>,
        network: NetworkName,
        client: Arc<dyn NetworkClient + Send + Sync>,
    ) -> Result<()> {
        debug!(target: "BRIDGE", "Add new client");

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
            debug!(target: "BRIDGE", "Start listening to new notification");
            let notification = self
                .notifiers
                .iter()
                .map(|n| n.recv())
                .collect::<FuturesUnordered<async_channel::Recv<TokenNotification>>>()
                .next()
                .await
                .map(|o| o.map_err(Error::from));

            debug!(target: "BRIDGE", "End listening to new notification");

            notification
        } else {
            debug!(target: "BRIDGE", "TEST");
            None
        }
    }

    pub async fn subscribe(
        self: Arc<Self>,
        drk_pub_key: jubjub::SubgroupPoint,
        mint: Option<String>,
    ) -> BridgeSubscribtion {
        debug!(target: "BRIDGE", "Start new subscription");
        let (sender, req) = async_channel::unbounded();
        let (rep, receiver) = async_channel::unbounded();

        smol::spawn(self.listen_for_new_subscription(req, rep, drk_pub_key, mint)).detach();

        BridgeSubscribtion { sender, receiver }
    }

    async fn listen_for_new_subscription(
        self: Arc<Self>,
        req: async_channel::Receiver<BridgeRequests>,
        rep: async_channel::Sender<BridgeResponse>,
        drk_pub_key: jubjub::SubgroupPoint,
        mint: Option<String>,
    ) -> Result<()> {
        debug!(target: "BRIDGE", "Listen for new subscription");
        let req = req.recv().await?;

        let network = req.network;

        if !self.clients.lock().await.contains_key(&network) {
            let res = BridgeResponse {
                error: BridgeResponseError::NotSupportedClient,
                payload: BridgeResponsePayload::Empty,
            };
            rep.send(res).await?;
            return Ok(());
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

        match req.payload {
            BridgeRequestsPayload::Watch(val) => match val {
                Some((private_key, public_key)) => {
                    let pub_key = client
                        .subscribe_with_keypair(private_key, public_key, drk_pub_key, mint_address)
                        .await?;
                    let res = BridgeResponse {
                        error: BridgeResponseError::NoError,
                        payload: BridgeResponsePayload::Address(pub_key),
                    };
                    rep.send(res).await?;
                }
                None => {
                    let sub = client.subscribe(drk_pub_key, mint_address).await?;
                    let res = BridgeResponse {
                        error: BridgeResponseError::NoError,
                        payload: BridgeResponsePayload::Watch(sub.secret_key, sub.public_key),
                    };
                    rep.send(res).await?;
                }
            },
            BridgeRequestsPayload::Send(addr, amount) => {
                client.send(addr, amount).await?;
                let res = BridgeResponse {
                    error: BridgeResponseError::NoError,
                    payload: BridgeResponsePayload::Send,
                };
                rep.send(res).await?;
            }
        }

        Ok(())
    }
}

#[async_trait]
pub trait NetworkClient {
    async fn subscribe(
        self: Arc<Self>,
        drk_pub_key: jubjub::SubgroupPoint,
        mint: Option<String>,
    ) -> Result<TokenSubscribtion>;

    // should check if the keypair in not already subscribed
    async fn subscribe_with_keypair(
        self: Arc<Self>,
        private_key: Vec<u8>,
        public_key: Vec<u8>,
        drk_pub_key: jubjub::SubgroupPoint,
        mint: Option<String>,
    ) -> Result<String>;

    async fn get_notifier(self: Arc<Self>) -> Result<async_channel::Receiver<TokenNotification>>;

    async fn send(self: Arc<Self>, address: Vec<u8>, amount: u64) -> Result<()>;
}
