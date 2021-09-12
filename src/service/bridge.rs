use crate::Result;

use async_executor::Executor;
use async_trait::async_trait;

use crate::serial::serialize;
use async_std::sync::{Arc, Mutex};
use std::collections::HashMap;

pub struct BridgeRequests {
    pub asset_id: jubjub::Fr,
    pub payload: BridgeRequestsPayload,
}

pub struct BridgeResponse {
    pub error: u64,
    pub payload: BridgeResponsePayload,
}

pub enum BridgeRequestsPayload {
    SendRequest(Vec<u8>, u64),
    WatchRequest,
}

pub enum BridgeResponsePayload {
    WatchResponse(Vec<u8>, Vec<u8>),
    SendResponse,
}

pub struct BridgeSubscribtion {
    pub sender: async_channel::Sender<BridgeRequests>,
    pub receiver: async_channel::Receiver<BridgeResponse>,
}

pub struct Bridge {
    clients: Mutex<HashMap<Vec<u8>, Arc<dyn CoinClient + Send + Sync>>>,
}
impl Bridge {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            clients: Mutex::new(HashMap::new()),
        })
    }

    pub async fn add_clients(
        self: Arc<Self>,
        asset_id: jubjub::Fr,
        client: Arc<dyn CoinClient + Send + Sync>,
    ) {
        let asset_id = serialize(&asset_id);
        self.clients.lock().await.insert(asset_id, client);
    }

    pub async fn subscribe(self: Arc<Self>, executor: Arc<Executor<'_>>) -> BridgeSubscribtion {
        let (sender, req) = async_channel::unbounded();
        let (rep, receiver) = async_channel::unbounded();

        executor
            .spawn(self.listen_for_new_subscribtion(req.clone(), rep.clone()))
            .detach();

        BridgeSubscribtion { sender, receiver }
    }

    pub async fn listen_for_new_subscribtion(
        self: Arc<Self>,
        req: async_channel::Receiver<BridgeRequests>,
        rep: async_channel::Sender<BridgeResponse>,
    ) -> Result<()> {
        let req = req.recv().await?;
        let asset_id = serialize(&req.asset_id);
        let client = &self.clients.lock().await[&asset_id];

        match req.payload {
            BridgeRequestsPayload::WatchRequest => {
                let (private, public) = client.watch().await?;
                let res = BridgeResponse {
                    error: 0,
                    payload: BridgeResponsePayload::WatchResponse(private, public),
                };
                rep.send(res).await?;
            }
            BridgeRequestsPayload::SendRequest(addr, amount) => {
                client.send(addr, amount).await?;
                let res = BridgeResponse {
                    error: 0,
                    payload: BridgeResponsePayload::SendResponse,
                };
                rep.send(res).await?;
            }
        }

        Ok(())
    }
}

#[async_trait]
pub trait CoinClient {
    // return private and public keys that be watching
    async fn watch(&self) -> Result<(Vec<u8>, Vec<u8>)>;
    async fn send(&self, address: Vec<u8>, amount: u64) -> Result<()>;
}
