use crate::rpc::{jsonrpc, jsonrpc::JsonResult};
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::Result;

use super::bridge::CoinClient;

use async_trait::async_trait;

use async_executor::Executor;
use futures::{SinkExt, StreamExt};
use log::*;
use rand::rngs::OsRng;
use serde::Serialize;
use serde_json::{json, Value};
use solana_client::{blockhash_query::BlockhashQuery, rpc_client::RpcClient};
use solana_sdk::{
    pubkey::Pubkey, signature::Signer, signer::keypair::Keypair, system_instruction,
    transaction::Transaction,
};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use async_std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::str::FromStr;

//const RPC_SERVER: &'static str = "https://api.mainnet-beta.solana.com";
//const WSS_SERVER: &'static str = "wss://api.mainnet-beta.solana.com";
const RPC_SERVER: &'static str = "https://api.devnet.solana.com";
const WSS_SERVER: &'static str = "wss://api.devnet.solana.com";
//const RPC_SERVER: &'static str = "http://localhost:8899";
//const WSS_SERVER: &'static str = "ws://localhost:8900";

#[derive(Serialize)]
struct SubscribeParams {
    encoding: Value,
    commitment: Value,
}

pub struct SolClient {
    keypair: Keypair,

    // subscription hashmap using pubkey as an index
    subscriptions: Arc<Mutex<HashMap<String, (Vec<u8>, u64)>>>,

    // notify when get new update
    notify_channel: (
        async_channel::Sender<(Vec<u8>, u64)>,
        async_channel::Receiver<(Vec<u8>, u64)>,
    ),

    // send subscription msg to websocket
    watch_channel: (
        async_channel::Sender<jsonrpc::JsonRequest>,
        async_channel::Receiver<jsonrpc::JsonRequest>,
    ),
}

impl SolClient {
    pub async fn new(keypair: Vec<u8>) -> Result<Arc<Self>> {
        let keypair: Keypair = deserialize(&keypair)?;

        let notify_channel = async_channel::unbounded();
        let watch_channel = async_channel::unbounded();

        Ok(Arc::new(Self {
            keypair,
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            notify_channel,
            watch_channel,
        }))
    }

    pub async fn subscribe_to_notify_channel(
        self: Arc<Self>,
    ) -> Result<async_channel::Receiver<(Vec<u8>, u64)>> {
        Ok(self.notify_channel.1.clone())
    }

    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        // WebSocket handshake/connect
        let (ws_stream, _) = connect_async(WSS_SERVER)
            .await
            .expect("Failed to connect to WebSocket server");

        let (mut write, read) = ws_stream.split();

        let self2 = self.clone();
        executor
            .spawn(async move {
                loop {
                    // Send the subscription request
                    let sub_msg = self2
                        .watch_channel
                        .1
                        .recv()
                        .await
                        .expect("receiving subscription msg");

                    write
                        .send(Message::Text(serde_json::to_string(&sub_msg).unwrap()))
                        .await
                        .unwrap();
                }
            })
            .detach();

        read.for_each(|message| async {
            let data = message.unwrap().into_text().unwrap();
            let v: JsonResult = serde_json::from_str(&data).unwrap();
            match v {
                JsonResult::Resp(r) => {
                    debug!(
                        target: "SOL BRIDGE",
                        "Successfully get response : {:?}",
                        r.result.as_i64().unwrap()
                    );
                    // let sub_id = r.result.as_i64().unwrap();
                }

                JsonResult::Err(e) => {
                    debug!(
                        target: "SOL BRIDGE",
                        "Error on subscription: {:?}", e.error.message.to_string());
                }

                JsonResult::Notif(n) => {
                    // TODO remove unwrap
                    let new_bal = n.params["result"]["value"]["lamports"].as_u64().unwrap();
                    let owner_pubkey = n.params["result"]["value"]["owner"].as_str().unwrap();
                    let (keypair, old_balance) =
                        self.subscriptions.lock().await[owner_pubkey].clone();

                    if new_bal > old_balance {
                        let sub_id = n.params["subscription"].as_u64().unwrap();
                        let received_balance = new_bal - old_balance;

                        let keypair: Keypair = deserialize(&keypair).expect("deserialize keypair");

                        self.send_to_main_account(keypair)
                            .expect("Send to main account");

                        self.notify_channel
                            .0
                            .send((
                                serialize(&Pubkey::from_str(owner_pubkey).unwrap()),
                                received_balance,
                            ))
                            .await
                            .expect("send notify msg");

                        SolClient::unsubscribe(self.watch_channel.0.clone(), sub_id)
                            .await
                            .unwrap();

                        debug!(
                            target: "SOL BRIDGE",
                            "Received {} lamports, to the pubkey: {} ",
                            received_balance, owner_pubkey.to_string(),
                        );
                    } else if new_bal < old_balance {
                        let sub_id = n.params["subscription"].as_u64().unwrap();
                        SolClient::unsubscribe(self.watch_channel.0.clone(), sub_id)
                            .await
                            .unwrap();
                    }
                }
            }
        })
        .await;
        Ok(())
    }

    fn send_to_main_account(&self, keypair: Keypair) -> Result<()> {
        let rpc = RpcClient::new(RPC_SERVER.to_string());

        let amount = rpc.get_balance(&keypair.pubkey()).unwrap();

        let instruction =
            system_instruction::transfer(&keypair.pubkey(), &self.keypair.pubkey(), amount);

        let mut tx = Transaction::new_with_payer(&[instruction], Some(&keypair.pubkey()));
        let bhq = BlockhashQuery::default();
        match bhq.get_blockhash_and_fee_calculator(&rpc, rpc.commitment()) {
            Err(_) => panic!("Couldn't connect to RPC"),
            Ok(v) => tx.sign(&[&keypair], v.0),
        }
        let _signature = rpc
            .send_and_confirm_transaction(&tx)
            .expect("send transaction");
        Ok(())
    }

    async fn unsubscribe(
        watch_channel_sender: async_channel::Sender<jsonrpc::JsonRequest>,
        sub_id: u64,
    ) -> Result<()> {
        let sub_msg = jsonrpc::request(json!("accountUnsubscribe"), json!([json!(sub_id)]));
        watch_channel_sender.send(sub_msg).await?;
        Ok(())
    }
}

#[async_trait]
impl CoinClient for SolClient {
    async fn watch(&self) -> Result<(Vec<u8>, Vec<u8>)> {
        let keypair = Keypair::generate(&mut OsRng);

        // Parameters for subscription to events related to `pubkey`.
        let sub_params = SubscribeParams {
            encoding: json!("jsonParsed"),
            // XXX: Use "finalized" for 100% certainty.
            commitment: json!("confirmed"),
        };

        let sub_msg = jsonrpc::request(
            json!("accountSubscribe"),
            json!([json!(keypair.pubkey().to_string()), json!(sub_params)]),
        );

        let rpc = RpcClient::new(RPC_SERVER.to_string());
        let balance = rpc.get_balance(&keypair.pubkey()).unwrap();

        self.subscriptions.lock().await.insert(
            keypair.pubkey().to_string(),
            (serialize(&keypair), balance),
        );

        self.watch_channel.0.send(sub_msg).await?;

        let pubkey = serialize(&keypair.pubkey());
        let keypair = serialize(&keypair);
        Ok((pubkey, keypair))
    }

    async fn send(&self, address: Vec<u8>, amount: u64) -> Result<()> {
        let rpc = RpcClient::new(RPC_SERVER.to_string());
        let address: Pubkey = deserialize(&address)?;
        let instruction = system_instruction::transfer(&self.keypair.pubkey(), &address, amount);

        let mut tx = Transaction::new_with_payer(&[instruction], Some(&self.keypair.pubkey()));
        let bhq = BlockhashQuery::default();
        match bhq.get_blockhash_and_fee_calculator(&rpc, rpc.commitment()) {
            Err(_) => panic!("Couldn't connect to RPC"),
            Ok(v) => tx.sign(&[&self.keypair], v.0),
        }

        let _signature = rpc
            .send_and_confirm_transaction(&tx)
            .expect("send transaction");
        Ok(())
    }
}

impl Encodable for Keypair {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let key = self.to_bytes();
        let len = key.encode(s)?;
        Ok(len)
    }
}

impl Decodable for Keypair {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let key: Vec<u8> = Decodable::decode(&mut d)?;
        let key = Keypair::from_bytes(key.as_slice()).map_err(|_| {
            crate::Error::from(SolFailed::DecodeAndEncodeError(
                "load keypair from slice".into(),
            ))
        })?;
        Ok(key)
    }
}

impl Encodable for Pubkey {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let key = self.to_string();
        let len = key.encode(s)?;
        Ok(len)
    }
}

impl Decodable for Pubkey {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        let key: String = Decodable::decode(&mut d)?;
        let key = Pubkey::try_from(key.as_str()).map_err(|_| {
            crate::Error::from(SolFailed::DecodeAndEncodeError(
                "load public key from slice".into(),
            ))
        })?;
        Ok(key)
    }
}

#[derive(Debug)]
pub enum SolFailed {
    NotEnoughValue(u64),
    BadSolAddress(String),
    SolError(String),
    DecodeAndEncodeError(String),
}

impl std::error::Error for SolFailed {}

impl std::fmt::Display for SolFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SolFailed::NotEnoughValue(i) => {
                write!(f, "There is no enough value {}", i)
            }
            SolFailed::BadSolAddress(ref err) => {
                write!(f, "Bad Sol Address: {}", err)
            }
            SolFailed::DecodeAndEncodeError(ref err) => {
                write!(f, "Decode and decode keys error: {}", err)
            }
            SolFailed::SolError(i) => {
                write!(f, "SolFailed: {}", i)
            }
        }
    }
}

impl From<crate::error::Error> for SolFailed {
    fn from(err: crate::error::Error) -> SolFailed {
        SolFailed::SolError(err.to_string())
    }
}

pub type SolResult<T> = std::result::Result<T, SolFailed>;
