use crate::rpc::{
    jsonrpc, jsonrpc::JsonError, jsonrpc::JsonNotification, jsonrpc::JsonRequest,
    jsonrpc::JsonResponse, jsonrpc::JsonResult, websockets::connect,
};
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::{Error, Result};

use super::bridge::{NetworkClient, TokenNotification, TokenSubscribtion};

use async_native_tls::TlsConnector;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
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
use std::convert::TryFrom;
use std::str::FromStr;
use tungstenite::Message;

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
    // subscriptions vecotr of puykey
    subscriptions: Arc<Mutex<Vec<Pubkey>>>,
    notify_channel: (
        async_channel::Sender<TokenNotification>,
        async_channel::Receiver<TokenNotification>,
    ),
}

impl SolClient {
    pub async fn new(keypair: Vec<u8>) -> Result<Arc<Self>> {
        let keypair: Keypair = deserialize(&keypair)?;

        let notify_channel = async_channel::unbounded();

        Ok(Arc::new(Self {
            keypair,
            subscriptions: Arc::new(Mutex::new(Vec::new())),
            notify_channel,
        }))
    }

    pub fn send_to_main_account(&self, keypair: &Keypair, mut amount: u64) -> SolResult<()> {
        debug!(
            target: "SOL BRIDGE",
            "sending received token to main account"
        );

        let rpc = RpcClient::new(RPC_SERVER.to_string());

        let fee = rpc
            .get_fees()
            .unwrap()
            .fee_calculator
            .lamports_per_signature;

        if fee >= amount {
            warn!(
                target: "SOL BRIDGE",
                "Received insufficient {} lamports, couldn't send it to
                the main_keypair",
                amount,
            );
            return Ok(());
        }

        // subtract fee from the new_bal
        amount -= fee;

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

    async fn handle_subscribe_request(self: Arc<Self>, keypair: Keypair) -> Result<()> {
        debug!(
            target: "SOL BRIDGE",
            "Handle subscribe request"
        );

        // check first if it's not already subscribed
        if self.subscriptions.lock().await.contains(&keypair.pubkey()) {
            return Ok(());
        }

        // Parameters for subscription to events related to `pubkey`.
        let sub_params = SubscribeParams {
            encoding: json!("jsonParsed"),
            commitment: json!("finalized"),
        };

        let sub_msg = jsonrpc::request(
            json!("accountSubscribe"),
            json!([json!(keypair.pubkey().to_string()), json!(sub_params)]),
        );

        let rpc = RpcClient::new(RPC_SERVER.to_string());
        let old_balance = rpc
            .get_balance(&keypair.pubkey())
            .map_err(|err| SolFailed::from(err))?;

        // WebSocket handshake/connect
        let builder = native_tls::TlsConnector::builder();
        let tls = TlsConnector::from(builder);
        let (stream, _) = connect(WSS_SERVER, tls).await?;


        let (mut write, mut read) = stream.split();

        let (unsubscribe_channel_sx, unsubscribe_channel_rv) = async_channel::unbounded();

        let unsubscribe_channel_rv2 = unsubscribe_channel_rv.clone();
        let ws_write_task: smol::Task<Result<()>> = smol::spawn(async move {
            write.send(Message::text(serde_json::to_string(&sub_msg)?))
                .await
                .map_err(|err| SolFailed::from(err))?;

            let unsub_msg = unsubscribe_channel_rv2.recv().await?;
            write
                .send(Message::text(serde_json::to_string(&unsub_msg)?))
                .await
                .map_err(|err| SolFailed::from(err))?;

            Ok(())
        });

        let keypair = serialize(&keypair);

        loop {
            let message = read.next().await;
            info!("msg: {:?}", message);
            if let Some(msg) = message {
                self.clone()
                    .read_ws_subscribe_msg(
                        msg,
                        &keypair,
                        old_balance,
                        unsubscribe_channel_sx.clone(),
                    )
                    .await?;
            } else {
                break;
            }
        }

        ws_write_task.cancel().await;
        Ok(())
    }

    async fn read_ws_subscribe_msg(
        &self,
        message: std::result::Result<Message, tungstenite::Error>,
        keypair: &Vec<u8>,
        old_balance: u64,
        unsubscribe_channel_sx: async_channel::Sender<JsonRequest>,
    ) -> SolResult<()> {
        let data = message?.into_text()?;

        let json_res: JsonResult;

        let v: std::collections::HashMap<String, Value> =
            serde_json::from_str(&data).map_err(|err| Error::from(err))?;

        // XXX this for testing
        if v.contains_key(&String::from("result")) {
            json_res = JsonResult::Resp(JsonResponse {
                jsonrpc: v["jsonrpc"].clone(),
                result: v["result"].clone(),
                id: v["id"].clone(),
            });
        } else if v.contains_key(&String::from("error")) {
            json_res = JsonResult::Err(JsonError {
                jsonrpc: v["jsonrpc"].clone(),
                error: serde_json::from_value(v["error"].clone()).unwrap(),
                id: v["id"].clone(),
            });
        } else {
            json_res = JsonResult::Notif(JsonNotification {
                jsonrpc: v["jsonrpc"].clone(),
                method: v["method"].clone(),
                params: v["params"].clone(),
            });
        }

        match json_res {
            JsonResult::Resp(r) => {
                // receive a response with subscription id
                let keypair: Keypair = deserialize(&keypair)?;
                match r.result.as_bool() {
                    Some(v) => {
                        if v {
                            debug!(
                                target: "SOL BRIDGE",
                                "Successfully unsubscribe from address {}",
                                keypair.pubkey(),
                            );
                        } else {
                            debug!(
                                target: "SOL BRIDGE",
                                "Unsuccessfully unsubscribe from address {}",
                                keypair.pubkey(),
                            );
                        }
                    }
                    None => {
                        self.subscriptions.lock().await.push(keypair.pubkey());
                        debug!(
                            target: "SOL BRIDGE",
                            "Successfully get response and subscribed to address {}",
                            keypair.pubkey(),
                        );
                    }
                }
            }

            JsonResult::Err(e) => {
                // receive an error
                debug!(
                    target: "SOL BRIDGE",
                    "Error on subscription: {:?}", e.error.message.to_string());
            }

            JsonResult::Notif(n) => {
                // receive notification once an account get updated
                debug!(
                    target: "SOL BRIDGE",
                    "receive new notification"
                );
                // get values from the notification
                let new_bal = n.params["result"]["value"]["lamports"]
                    .as_u64()
                    .ok_or(Error::ParseIntError)?;

                let sub_id = n.params["subscription"]
                    .as_u64()
                    .ok_or(Error::ParseIntError)?;

                let keypair: Keypair = deserialize(&keypair)?;

                match new_bal > old_balance {
                    true => {
                        let received_balance = new_bal - old_balance;

                        self.notify_channel
                            .0
                            .send(TokenNotification {
                                secret_key: serialize(&keypair),
                                received_balance,
                            })
                            .await
                            .map_err(|err| Error::from(err))?;

                        self.unsubscribe(sub_id, &keypair.pubkey(), unsubscribe_channel_sx.clone())
                            .await?;

                        debug!(
                            target: "SOL BRIDGE",
                            "Received {} lamports, to the pubkey: {} ",
                            received_balance, keypair.pubkey().to_string(),
                        );

                        self.send_to_main_account(&keypair, new_bal)?;
                    }
                    false => {
                        self.unsubscribe(sub_id, &keypair.pubkey(), unsubscribe_channel_sx.clone())
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn unsubscribe(
        &self,
        sub_id: u64,
        pubkey: &Pubkey,
        unsubscribe_channel_sx: async_channel::Sender<JsonRequest>,
    ) -> Result<()> {
        let sub_msg = jsonrpc::request(json!("accountUnsubscribe"), json!([sub_id]));

        unsubscribe_channel_sx.send(sub_msg).await?;

        let index = self
            .subscriptions
            .lock()
            .await
            .iter()
            .position(|p| p == pubkey);

        if let Some(ind) = index {
            self.subscriptions.lock().await.remove(ind);
        }

        Ok(())
    }
}

#[async_trait]
impl NetworkClient for SolClient {
    async fn subscribe(self: Arc<Self>) -> Result<TokenSubscribtion> {
        let keypair = Keypair::generate(&mut OsRng);

        let public_key = keypair.pubkey().to_string();
        let secret_key = serialize(&keypair);

        let self2 = self.clone();
        smol::spawn(self2.handle_subscribe_request(keypair)).detach();

        Ok(TokenSubscribtion {
            secret_key,
            public_key,
        })
    }

    // in solana case private key it's the same as keypair
    async fn subscribe_with_keypair(
        self: Arc<Self>,
        private_key: Vec<u8>,
        _public_key: Vec<u8>,
    ) -> Result<String> {
        let keypair: Keypair = deserialize(&private_key)?;

        let public_key = keypair.pubkey().to_string();

        let self2 = self.clone();
        smol::spawn(self2.handle_subscribe_request(keypair)).detach();

        Ok(public_key)
    }

    async fn get_notifier(self: Arc<Self>) -> Result<async_channel::Receiver<TokenNotification>> {
        Ok(self.notify_channel.1.clone())
    }

    async fn send(self: Arc<Self>, address: Vec<u8>, amount: u64) -> Result<()> {
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
pub fn get_associated_token_account(owner: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
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
pub fn account_is_initialized_mint(mint: &Pubkey) -> bool {
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
