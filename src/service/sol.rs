use std::convert::TryFrom;
use std::str::FromStr;

use async_native_tls::TlsConnector;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use log::{debug, error, warn};
use rand::rngs::OsRng;
use serde::Serialize;
use serde_json::{json, Value};
use solana_client::{blockhash_query::BlockhashQuery, rpc_client::RpcClient};
use solana_sdk::{
    native_token::lamports_to_sol, program_pack::Pack, pubkey::Pubkey, signature::Signer,
    signer::keypair::Keypair, system_instruction, transaction::Transaction,
};
use tungstenite::Message;

use crate::rpc::{jsonrpc, jsonrpc::JsonResult, websockets};
use crate::serial::{deserialize, serialize, Decodable, Encodable};
use crate::{Error, Result};

use super::bridge::{NetworkClient, TokenNotification, TokenSubscribtion};

#[derive(Serialize)]
struct SubscribeParams {
    encoding: Value,
    commitment: Value,
}

pub struct SolClient {
    keypair: Keypair,
    // Subscriptions vector of pubkey
    subscriptions: Arc<Mutex<Vec<Pubkey>>>,
    notify_channel: (
        async_channel::Sender<TokenNotification>,
        async_channel::Receiver<TokenNotification>,
    ),
    rpc_server: &'static str,
    wss_server: &'static str,
}

impl SolClient {
    pub async fn new(keypair: Vec<u8>, network: &str) -> Result<Arc<Self>> {
        let keypair: Keypair = deserialize(&keypair)?;
        let notify_channel = async_channel::unbounded();

        let (rpc_server, wss_server) = match network {
            "mainnet" => (
                "https://api.mainnet-beta.solana.com",
                "wss://api.devnet.solana.com",
            ),
            "devnet" => (
                "https://api.devnet.solana.com",
                "wss://api.devnet.solana.com",
            ),
            "testnet" => (
                "https://api.testnet.solana.com",
                "wss://api.testnet.solana.com",
            ),
            "localhost" => ("http://localhost:8899", "ws://localhost:8900"),
            _ => return Err(Error::NotSupportedNetwork),
        };

        Ok(Arc::new(Self {
            keypair,
            subscriptions: Arc::new(Mutex::new(Vec::new())),
            notify_channel,
            rpc_server,
            wss_server,
        }))
    }

    // TODO: Make this function more robust. Currently we just call it
    // and put it in the background. This means no errors are actually
    // handled, and it just fails silently.
    async fn handle_subscribe_request(
        self: Arc<Self>,
        keypair: Keypair,
        is_token: bool,
    ) -> Result<()> {
        debug!(target: "SOL BRIDGE", "handle_subscribe_request()");

        // Check if we're already subscribed
        if self.subscriptions.lock().await.contains(&keypair.pubkey()) {
            return Ok(());
        }

        let rpc = RpcClient::new(self.rpc_server.to_string());

        // Fetch the current balance.
        let prev_balance = if !is_token {
            rpc.get_balance(&keypair.pubkey())
                .map_err(|err| SolFailed::from(err))?
        } else {
            // TODO: SPL Token balance
            0
        };
        let mut cur_balance = prev_balance;
        let mut decimals: Option<u64> = None;
        let mut mint: Option<&str> = None;

        // WebSocket connection
        let builder = native_tls::TlsConnector::builder();
        let tls = TlsConnector::from(builder);
        let (mut stream, _) = websockets::connect(self.wss_server, tls).await?;

        // Subscription request build
        let sub_params = SubscribeParams {
            encoding: json!("jsonParsed"),
            commitment: json!("finalized"),
        };

        let subscription = jsonrpc::request(
            json!("accountSubscribe"),
            json!([json!(keypair.pubkey().to_string()), json!(sub_params)]),
        );

        debug!(target: "SOLANA RPC", "--> {}", serde_json::to_string(&subscription)?);
        stream
            .send(Message::text(serde_json::to_string(&subscription)?))
            .await?;

        // Declare params here for longer variable lifetime.
        let params: Value;

        // Subscription ID used for unsubscribing later.
        let mut sub_id: i64 = 0;

        loop {
            let message = stream.next().await.ok_or_else(|| Error::TungsteniteError)?;
            let message = message.unwrap();
            debug!(target: "SOLANA SUBSCRIPTION", "<-- {}", message.clone().into_text()?);

            match serde_json::from_slice(&message.into_data())? {
                JsonResult::Resp(r) => {
                    // ACK
                    debug!(target: "SOLANA RPC", "<-- {}", serde_json::to_string(&r)?);
                    self.subscriptions.lock().await.push(keypair.pubkey());
                    sub_id = r.result.as_i64().unwrap();
                }
                JsonResult::Err(e) => {
                    debug!(target: "SOLANA RPC", "<-- {}", serde_json::to_string(&e)?);
                    // TODO: Try removing pubkey from subscriptions here?
                    return Err(Error::JsonRpcError(e.error.message.to_string()));
                }
                JsonResult::Notif(n) => {
                    // Account updated
                    debug!(target: "SOLANA RPC", "Got WebSocket notification");
                    params = n.params["result"]["value"].clone();

                    if is_token {
                        cur_balance = params["data"]["info"]["tokenAmount"]["amount"]
                            .as_u64()
                            .unwrap();

                        decimals = Some(
                            params["data"]["info"]["tokenAmount"]["decimals"]
                                .as_u64()
                                .unwrap(),
                        );

                        mint = Some(params["data"]["info"]["mint"].as_str().unwrap());
                    } else {
                        cur_balance = params["lamports"].as_u64().unwrap();
                        decimals = None;
                        mint = None;
                    }
                    break;
                }
            }
        }

        // I miss goto/defer.
        let index = self
            .subscriptions
            .lock()
            .await
            .iter()
            .position(|p| p == &keypair.pubkey());
        if let Some(ind) = index {
            debug!("Removing subscription from list");
            self.subscriptions.lock().await.remove(ind);
        }

        let unsubscription = jsonrpc::request(json!("accountUnsubscribe"), json!([sub_id]));
        stream
            .send(Message::text(serde_json::to_string(&unsubscription)?))
            .await?;

        if cur_balance - prev_balance <= 0 {
            error!("Current balance is not positive");
            return Err(Error::ServicesError("Current balance is not positive"));
        }

        if is_token {
            debug!(target: "SOL BRIDGE", "Received {} {:?} tokens",
                (cur_balance - prev_balance) * decimals.unwrap(), mint.unwrap());
            self.send_tok_to_main_wallet(mint.unwrap(), cur_balance, keypair)
        } else {
            debug!(target: "SOL BRIDGE", "Received {} SOL", lamports_to_sol(cur_balance - prev_balance));
            self.send_sol_to_main_wallet(cur_balance, &keypair)
        }
    }

    // TODO
    fn send_tok_to_main_wallet(
        self: Arc<Self>,
        mint: &str,
        amount: u64,
        keypair: Keypair,
    ) -> Result<()> {
        debug!(target: "SOL BRIDGE", "Sending tokens to main wallet");
        Ok(())
    }

    fn send_sol_to_main_wallet(self: Arc<Self>, amount: u64, keypair: &Keypair) -> Result<()> {
        debug!(target: "SOL BRIDGE", "Sending {} SOL to main wallet", lamports_to_sol(amount));

        let rpc = RpcClient::new(self.rpc_server.to_string());

        let fee = rpc
            .get_fees()
            .unwrap()
            .fee_calculator
            .lamports_per_signature;

        if fee >= amount {
            warn!(target: "SOL BRIDGE", "Insufficient funds on {:?} to send tx", &keypair.pubkey());
            return Ok(());
        }

        let amnt_to_transfer = amount - fee;

        let ix = system_instruction::transfer(
            &keypair.pubkey(),
            &self.keypair.pubkey(),
            amnt_to_transfer,
        );

        let mut tx = Transaction::new_with_payer(&[ix], Some(&keypair.pubkey()));
        let bhq = BlockhashQuery::default();
        match bhq.get_blockhash_and_fee_calculator(&rpc, rpc.commitment()) {
            Err(_) => panic!("Couldn't connect to RPC"),
            Ok(v) => tx.sign(&[keypair], v.0),
        }

        let signature = rpc.send_and_confirm_transaction(&tx);
        debug!(target: "SOL BRIDGE", "Sent to main wallet: {}", signature.unwrap());

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
        // TODO: true/false depending on is_token
        smol::spawn(self2.handle_subscribe_request(keypair, false)).detach();

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
        // TODO: true/false depending on is_token
        smol::spawn(self2.handle_subscribe_request(keypair, false)).detach();

        Ok(public_key)
    }

    async fn get_notifier(self: Arc<Self>) -> Result<async_channel::Receiver<TokenNotification>> {
        Ok(self.notify_channel.1.clone())
    }

    async fn send(self: Arc<Self>, address: Vec<u8>, amount: u64) -> Result<()> {
        let rpc = RpcClient::new(self.rpc_server.to_string());
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

/// Gets account token balance for given mint.
/// Returns: (amount, decimals)
pub fn get_account_token_balance(
    rpc_server: String,
    address: &Pubkey,
    mint: &Pubkey,
) -> SolResult<(u64, u64)> {
    let rpc = RpcClient::new(rpc_server);

    let mint_account = rpc.get_account(mint)?;
    let token_account = rpc.get_account(address)?;
    let mint_data = spl_token::state::Mint::unpack_from_slice(&mint_account.data)?;
    let token_data = spl_token::state::Account::unpack_from_slice(&token_account.data)?;

    Ok((token_data.amount, mint_data.decimals as u64))
}

/// Check if given account is a valid token mint
pub fn account_is_initialized_mint(rpc_server: String, mint: &Pubkey) -> bool {
    let rpc = RpcClient::new(rpc_server);
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

impl From<solana_sdk::program_error::ProgramError> for SolFailed {
    fn from(err: solana_sdk::program_error::ProgramError) -> SolFailed {
        SolFailed::SolError(err.to_string())
    }
}

impl From<crate::error::Error> for SolFailed {
    fn from(err: crate::error::Error) -> SolFailed {
        SolFailed::SolError(err.to_string())
    }
}

pub type SolResult<T> = std::result::Result<T, SolFailed>;
