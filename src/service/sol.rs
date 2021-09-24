use crate::rpc::{jsonrpc, jsonrpc::JsonResult};
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::{Error, Result};

use super::bridge::{TokenClient, TokenNotification, TokenSubscribtion};

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
use tokio_tungstenite::{connect_async, tungstenite, tungstenite::protocol::Message};

use async_std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::str::FromStr;

//const RPC_SERVER: &str = "https://api.mainnet-beta.solana.com";
//const WSS_SERVER: &str = "wss://api.mainnet-beta.solana.com";
const RPC_SERVER: &str = "https://api.devnet.solana.com";
const WSS_SERVER: &str = "wss://api.devnet.solana.com";
//const RPC_SERVER: &str = "http://localhost:8899";
//const WSS_SERVER: &str = "ws://localhost:8900";

#[derive(Serialize)]
struct SubscribeParams {
    encoding: Value,
    commitment: Value,
}

pub struct SolClient {
    keypair: Keypair,

    // subscriptions hashmap using pubkey as an index and a value of (keypair, amount)
    subscriptions: Arc<Mutex<HashMap<Pubkey, (Keypair, u64)>>>,

    notify_channel: (
        async_channel::Sender<TokenNotification>,
        async_channel::Receiver<TokenNotification>,
    ),

    subscribe_channel: (
        async_channel::Sender<jsonrpc::JsonRequest>,
        async_channel::Receiver<jsonrpc::JsonRequest>,
    ),
}

impl SolClient {
    pub async fn new(keypair: Vec<u8>) -> Result<Arc<Self>> {
        let keypair: Keypair = deserialize(&keypair)?;

        let notify_channel = async_channel::unbounded();
        let subscribe_channel = async_channel::unbounded();

        Ok(Arc::new(Self {
            keypair,
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            notify_channel,
            subscribe_channel,
        }))
    }

    pub async fn run(self: Arc<Self>, executor: Arc<Executor<'_>>) -> SolResult<()> {
        // WebSocket handshake/connect
        let (ws_stream, _) = connect_async(WSS_SERVER).await?;

        let (mut write, read) = ws_stream.split();

        let self2 = self.clone();
        let _: async_executor::Task<Result<()>> = executor.spawn(async move {
            loop {
                // recv a request for websocket
                let sub_msg = self2.subscribe_channel.1.recv().await?;

                // write the request to websocket
                write
                    .send(Message::Text(serde_json::to_string(&sub_msg)?))
                    .await
                    .map_err(|err| SolFailed::from(err))?;
            }
        });

        read.for_each(|message| async {
            // read ws msg
            self.clone()
                .read_ws_msg(message)
                .await
                .expect("read from websocket");
        })
        .await;
        Ok(())
    }

    async fn read_ws_msg(
        self: Arc<Self>,
        message: std::result::Result<Message, tungstenite::Error>,
    ) -> SolResult<()> {
        let data = message?.into_text()?;

        let v: JsonResult = serde_json::from_str(&data).map_err(|err| Error::from(err))?;

        match v {
            JsonResult::Resp(r) => {
                // receive a response with subscription id
                let sub_id = r.result.as_i64().ok_or(Error::ParseIntError)?;
                debug!(
                    target: "SOL BRIDGE",
                    "Successfully get response : {:?}",
                    sub_id
                );
            }

            JsonResult::Err(e) => {
                // receive an error
                debug!(
                        target: "SOL BRIDGE",
                        "Error on subscription: {:?}", e.error.message.to_string());
            }

            JsonResult::Notif(n) => {
                // receive notification once an account get updated

                // get values from the notification
                let new_bal = n.params["result"]["value"]["lamports"]
                    .as_u64()
                    .ok_or(Error::ParseIntError)?;

                let owner_pubkey = n.params["result"]["value"]["owner"]
                    .as_str()
                    .ok_or(Error::ParseFailed("Error Parse serde_json Value to &str"))?;

                let owner_pubkey: Pubkey = Pubkey::from_str(&owner_pubkey)?;

                let sub_id = n.params["subscription"]
                    .as_u64()
                    .ok_or(Error::ParseIntError)?;

                // get the keypair and old_balance from the subscriptions list
                let (keypair, old_balance) = &self.subscriptions.lock().await[&owner_pubkey];

                match new_bal > *old_balance {
                    true => {
                        let received_balance = new_bal - old_balance;

                        self.send_to_main_account(&keypair)?;

                        self.notify_channel
                            .0
                            .send(TokenNotification {
                                secret_key: serialize(keypair),
                                received_balance,
                            })
                            .await
                            .map_err(|err| Error::from(err))?;

                        self.unsubscribe(sub_id, &owner_pubkey).await?;

                        debug!(
                            target: "SOL BRIDGE",
                            "Received {} lamports, to the pubkey: {} ",
                            received_balance, owner_pubkey.to_string(),
                        );
                    }
                    false => {
                        self.unsubscribe(sub_id, &owner_pubkey).await?;
                    }
                }
            }
        }
        Ok(())
    }

    fn send_to_main_account(&self, keypair: &Keypair) -> SolResult<()> {
        let rpc = RpcClient::new(RPC_SERVER.to_string());

        let amount = rpc.get_balance(&keypair.pubkey())?;

        let instruction =
            system_instruction::transfer(&keypair.pubkey(), &self.keypair.pubkey(), amount);

        let mut tx = Transaction::new_with_payer(&[instruction], Some(&keypair.pubkey()));
        let bhq = BlockhashQuery::default();
        match bhq.get_blockhash_and_fee_calculator(&rpc, rpc.commitment()) {
            Err(_) => panic!("Couldn't connect to RPC"),
            Ok(v) => tx.sign(&[keypair], v.0),
        }
        let _signature = rpc.send_and_confirm_transaction(&tx)?;
        Ok(())
    }

    async fn unsubscribe(&self, sub_id: u64, pubkey: &Pubkey) -> Result<()> {
        let sub_msg = jsonrpc::request(json!("accountUnsubscribe"), json!([json!(sub_id)]));
        self.subscribe_channel.0.send(sub_msg).await?;
        self.subscriptions.lock().await.remove(pubkey);
        Ok(())
    }
}

#[async_trait]
impl TokenClient for SolClient {
    async fn subscribe(&self) -> Result<TokenSubscribtion> {
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
        let balance = rpc
            .get_balance(&keypair.pubkey())
            .map_err(|err| SolFailed::from(err))?;

        let public_key = keypair.pubkey().to_string();
        // NOTE we send keypair for sol as secret_key
        let secret_key = serialize(&keypair);

        // add to subscriptions list
        self.subscriptions
            .lock()
            .await
            .insert(keypair.pubkey(), (keypair, balance));

        //  send
        self.subscribe_channel.0.send(sub_msg).await?;

        Ok(TokenSubscribtion {
            secret_key,
            public_key,
        })
    }

    async fn get_notifier(&self) -> Result<async_channel::Receiver<TokenNotification>> {
        Ok(self.notify_channel.1.clone())
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
            .map_err(|err| SolFailed::from(err))?;

        Ok(())
    }
}

/// Derive an associated token address from given owner and mint
fn get_associated_token_account(owner: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {

    let associated_token =
        Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").unwrap();

    Pubkey::find_program_address(
        &[
            &owner.to_bytes(),
            &spl_token::id().to_bytes(),
            &mint.to_bytes(),
        ],
        &associated_token,
    )
}

/// Check if given account is a valid token mint
fn account_is_initialized_mint(mint: &Pubkey) -> bool {
    let rpc = RpcClient::new(RPC_SERVER.to_string());
    match rpc.get_token_supply(mint) {
        Ok(_) => return true,
        Err(_) => return false,
    }
}

impl Encodable for Keypair {
    fn encode<S: std::io::Write>(&self, s: S) -> Result<usize> {
        let key: Vec<u8> = self.to_bytes().to_vec();
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
    DecodeAndEncodeError(String),
    WebSocketError(String),
    SolClientError(String),
    ParseError(String),
    SolError(String),
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
            SolFailed::WebSocketError(i) => {
                write!(f, "WebSocket Error: {}", i)
            }
            SolFailed::ParseError(i) => {
                write!(f, "Parse Error: {}", i)
            }
            SolFailed::SolClientError(i) => {
                write!(f, "Solana Client Error: {}", i)
            }
            SolFailed::SolError(i) => {
                write!(f, "SolFailed: {}", i)
            }
        }
    }
}

impl From<solana_sdk::pubkey::ParsePubkeyError> for SolFailed {
    fn from(err: solana_sdk::pubkey::ParsePubkeyError) -> SolFailed {
        SolFailed::ParseError(err.to_string())
    }
}

impl From<tungstenite::Error> for SolFailed {
    fn from(err: tungstenite::Error) -> SolFailed {
        SolFailed::WebSocketError(err.to_string())
    }
}

impl From<solana_client::client_error::ClientError> for SolFailed {
    fn from(err: solana_client::client_error::ClientError) -> SolFailed {
        SolFailed::SolError(err.to_string())
    }
}

impl From<crate::error::Error> for SolFailed {
    fn from(err: crate::error::Error) -> SolFailed {
        SolFailed::SolError(err.to_string())
    }
}

pub type SolResult<T> = std::result::Result<T, SolFailed>;


