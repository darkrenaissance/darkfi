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

use std::str::FromStr;

use async_executor::Executor;
use async_native_tls::TlsConnector;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use log::{debug, error, info, trace, warn};
use serde::Serialize;
use serde_json::{json, Value};
use solana_client::{blockhash_query::BlockhashQuery, rpc_client::RpcClient};
use solana_sdk::{
    native_token::{lamports_to_sol, sol_to_lamports},
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{Signature, Signer},
    signer::keypair::Keypair,
    system_instruction,
    transaction::Transaction,
};
use spl_associated_token_account::{create_associated_token_account, get_associated_token_address};
use tungstenite::Message;

use super::bridge::{NetworkClient, TokenNotification, TokenSubscribtion};

use darkfi::{
    crypto::{keypair::PublicKey, token_id::generate_id2},
    rpc::{jsonrpc, jsonrpc::JsonResult, websockets, websockets::WsStream},
    util::{
        expand_path, load_keypair_to_str,
        parse::truncate,
        serial::{deserialize, serialize, Decodable, Encodable},
        sleep, NetworkName,
    },
    wallet::cashierdb::{CashierDb, TokenKey},
    Error, Result,
};

pub const SOL_NATIVE_TOKEN_ID: &str = "So11111111111111111111111111111111111111112";

struct SolKeypair(Keypair);
struct SolPubkey(Pubkey);

#[derive(Serialize)]
struct SubscribeParams {
    encoding: Value,
    commitment: Value,
}

pub struct SolClient {
    main_keypair: Keypair,
    // Subscriptions vector of pubkey
    subscriptions: Arc<Mutex<Vec<Pubkey>>>,
    notify_channel:
        (async_channel::Sender<TokenNotification>, async_channel::Receiver<TokenNotification>),
    rpc_server: &'static str,
    wss_server: &'static str,
}

impl SolClient {
    pub async fn new(
        cashier_wallet: Arc<CashierDb>,
        network: &str,
        keypair_path: &str,
    ) -> Result<Arc<Self>> {
        let notify_channel = async_channel::unbounded();

        let main_keypair: SolKeypair;

        let main_keypairs = cashier_wallet.get_main_keys(&NetworkName::Solana).await?;

        if keypair_path.is_empty() {
            if main_keypairs.is_empty() {
                main_keypair = SolKeypair(Keypair::new());
                cashier_wallet
                    .put_main_keys(
                        &TokenKey {
                            secret_key: serialize(&main_keypair),
                            public_key: serialize(&SolPubkey(main_keypair.0.pubkey())),
                        },
                        &NetworkName::Solana,
                    )
                    .await?;
            } else {
                main_keypair =
                    deserialize::<SolKeypair>(&main_keypairs[main_keypairs.len() - 1].secret_key)?;
            }
        } else {
            let keypair_str = load_keypair_to_str(expand_path(keypair_path)?)?;

            let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_str)?;
            main_keypair = SolKeypair(
                Keypair::from_bytes(&keypair_bytes)
                    .map_err(|e| SolFailed::Signature(e.to_string()))?,
            );
        }

        info!(target: "SOL BRIDGE", "Main SOL wallet pubkey: {:?}", &main_keypair.0.pubkey());

        let (rpc_server, wss_server) = match network {
            "mainnet" => ("https://api.mainnet-beta.solana.com", "wss://api.devnet.solana.com"),
            "devnet" => ("https://api.devnet.solana.com", "wss://api.devnet.solana.com"),
            "testnet" => ("https://api.testnet.solana.com", "wss://api.testnet.solana.com"),
            "localhost" => ("http://localhost:8899", "ws://localhost:8900"),
            _ => return Err(Error::UnsupportedCoinNetwork),
        };

        Ok(Arc::new(Self {
            main_keypair: main_keypair.0,
            subscriptions: Arc::new(Mutex::new(Vec::new())),
            notify_channel,
            rpc_server,
            wss_server,
        }))
    }

    fn check_main_account_balance(&self, rpc: &RpcClient) -> SolResult<bool> {
        let main_sol_balance =
            rpc.get_balance(&self.main_keypair.pubkey()).map_err(SolFailed::from)?;

        // 0.0001 is the maximum that could happen
        let lamports_per_signature = sol_to_lamports(0.0001);
        let required_funds = lamports_per_signature * 3;

        Ok(main_sol_balance > required_funds)
    }

    async fn handle_subscribe_request(
        self: Arc<Self>,
        keypair: Keypair,
        drk_pub_key: PublicKey,
        mint: Option<Pubkey>,
    ) -> SolResult<()> {
        trace!(target: "SOL BRIDGE", "handle_subscribe_request()");

        // Derive token pubkey if mint was provided.
        let pubkey = if mint.is_some() {
            get_associated_token_address(&keypair.pubkey(), &mint.unwrap())
        } else {
            keypair.pubkey()
        };

        if mint.is_some() {
            debug!(target: "SOL BRIDGE", "Got subscribe request for SPL token");
            debug!(target: "SOL BRIDGE", "Main wallet: {}", keypair.pubkey());
            debug!(target: "SOL BRIDGE", "Associated token address: {}", pubkey);
        } else {
            debug!(target: "SOL BRIDGE", "Got subscribe request for native SOL");
            debug!(target: "SOL BRIDGE", "Main wallet: {}", keypair.pubkey());
        }

        // Check if we're already subscribed
        if self.subscriptions.lock().await.contains(&pubkey) {
            return Ok(())
        }

        let rpc = RpcClient::new(self.rpc_server.to_string());

        // Fetch the current balance.
        let (prev_balance, decimals) = if mint.is_none() {
            (rpc.get_balance(&pubkey).map_err(SolFailed::from)?, 9)
        } else {
            let mint = mint.unwrap();
            match get_account_token_balance(&rpc, &pubkey, &mint) {
                Ok(v) => v,
                Err(_) => {
                    let (exists, decimals) = account_is_initialized_mint(&rpc, &mint);
                    if !exists {
                        debug!("Could not figure out the number of decimals in SPL token");
                        return Err(SolFailed::MintIsNotValid(mint.to_string()))
                    }
                    (0, decimals)
                }
            }
        };

        // WebSocket connection
        let builder = native_tls::TlsConnector::builder();
        let tls = TlsConnector::from(builder);
        let (stream, _) = websockets::connect(self.wss_server, tls).await?;
        let (mut write, mut read) = stream.split();

        // Subscription request build
        let sub_params =
            SubscribeParams { encoding: json!("jsonParsed"), commitment: json!("finalized") };

        let subscription = jsonrpc::request(
            json!("accountSubscribe"),
            json!([json!(pubkey.to_string()), json!(sub_params)]),
        );

        debug!(target: "SOLANA RPC", "--> {}", serde_json::to_string(&subscription)?);
        write.send(Message::text(serde_json::to_string(&subscription)?)).await?;

        // Subscription ID used for unsubscribing later.
        let mut sub_id: i64 = 0;

        // The balance we are going to receive from the JSONRPC notification
        let cur_balance: u64;

        let ping_payload: Vec<u8> = vec![42, 33, 31, 42];

        let iter_interval = 1;
        let mut sub_iter = 0;

        loop {
            let message = read
                .next()
                .await
                .ok_or_else(|| Error::TungsteniteError("No more messages".to_string()))??;

            if let Message::Pong(_) = message.clone() {
                if sub_iter > 60 * 10 {
                    // 10 minutes
                    self.unsubscribe(&mut write, &pubkey, &sub_id).await?;
                    return Err(SolFailed::RpcError(format!("Deposit for {:?} expired", pubkey)))
                }
                sub_iter += iter_interval;
                sleep(iter_interval).await;
                write.send(Message::Ping(ping_payload.clone())).await?;
                continue
            };

            match serde_json::from_slice(&message.into_data())? {
                JsonResult::Resp(r) => {
                    // ACK
                    debug!(target: "SOLANA RPC", "<-- {}", serde_json::to_string(&r)?);
                    self.subscriptions.lock().await.push(pubkey);
                    sub_id = r.result.as_i64().unwrap();

                    // Start sending pings
                    write.send(Message::Ping(ping_payload.clone())).await?;
                }
                JsonResult::Err(e) => {
                    debug!(target: "SOLANA RPC", "<-- {}", serde_json::to_string(&e)?);

                    self.unsubscribe(&mut write, &pubkey, &sub_id).await?;
                    return Err(SolFailed::RpcError(e.error.message.to_string()))
                }
                JsonResult::Notif(n) => {
                    // Account updated
                    debug!(target: "SOLANA RPC", "Got WebSocket notification");
                    let params = n.params["result"]["value"].clone();

                    if mint.is_some() {
                        cur_balance = params["data"]["parsed"]["info"]["tokenAmount"]["amount"]
                            .as_str()
                            .unwrap()
                            .parse()
                            .map_err(Error::from)?;
                    } else {
                        cur_balance = params["lamports"].as_u64().unwrap();
                    }
                    break
                }
            }
        }

        let send_notification = self.notify_channel.0.clone();

        let self2 = self.clone();
        self2.unsubscribe(&mut write, &pubkey, &sub_id).await?;

        if cur_balance < prev_balance {
            return Err(SolFailed::Notification("New balance is less than previous balance".into()))
        }

        let amnt = cur_balance - prev_balance;

        if mint.is_some() {
            let ui_amnt = amnt / u64::pow(10, decimals as u32);

            send_notification
                .send(TokenNotification {
                    network: NetworkName::Solana,
                    token_id: generate_id2(&mint.unwrap().to_string(), &NetworkName::Solana)?,
                    drk_pub_key,
                    received_balance: amnt,
                    decimals: decimals as u16,
                })
                .await
                .map_err(Error::from)?;

            info!(target: "SOL BRIDGE", "Received {} {:?} tokens", ui_amnt, mint.unwrap());
            let _ = self.send_tok_to_main_wallet(&rpc, &mint.unwrap(), amnt, decimals, &keypair)?;
        } else {
            let ui_amnt = lamports_to_sol(amnt);

            send_notification
                .send(TokenNotification {
                    network: NetworkName::Solana,
                    token_id: generate_id2(SOL_NATIVE_TOKEN_ID, &NetworkName::Solana)?,
                    drk_pub_key,
                    received_balance: amnt,
                    decimals: decimals as u16,
                })
                .await
                .map_err(Error::from)?;

            info!(target: "SOL BRIDGE", "Received {} SOL", ui_amnt);
            let _ = self.send_sol_to_main_wallet(&rpc, amnt, &keypair)?;
        }

        Ok(())
    }

    async fn unsubscribe(
        self: Arc<Self>,
        write: &mut futures::stream::SplitSink<WsStream, tungstenite::Message>,
        pubkey: &Pubkey,
        sub_id: &i64,
    ) -> Result<()> {
        {
            let mut subscriptions = self.subscriptions.lock().await;
            let index = subscriptions.iter().position(|p| p == pubkey);
            if let Some(ind) = index {
                trace!(target: "SOL BRIDGE", "Removing subscription from list");
                subscriptions.remove(ind);
            }
        }

        let unsubscription = jsonrpc::request(json!("accountUnsubscribe"), json!([sub_id]));

        write.send(Message::text(serde_json::to_string(&unsubscription)?)).await?;

        Ok(())
    }

    fn send_tok_to_main_wallet(
        self: Arc<Self>,
        rpc: &RpcClient,
        mint: &Pubkey,
        amount: u64,
        decimals: u64,
        keypair: &Keypair,
    ) -> SolResult<Signature> {
        debug!(target: "SOL BRIDGE", "Sending {} {:?} tokens to main wallet",
            amount / u64::pow(10, decimals as u32), mint);

        // The token account from our main wallet
        let main_tok_pk = get_associated_token_address(&self.main_keypair.pubkey(), mint);
        // The token account from the deposit wallet
        let temp_tok_pk = get_associated_token_address(&keypair.pubkey(), mint);

        let mut instructions = vec![];

        match rpc.get_account_data(&main_tok_pk) {
            Ok(v) => {
                // This will fail in the event of unexpected data
                // otherwise it's valid token data, and we consider account initialized.
                spl_token::state::Account::unpack_from_slice(&v)?;
            }
            Err(_) => {
                // Unitinialized, so we add a creation instruction
                debug!("Main wallet token account is uninitialized. Adding init instruction.");
                let init_ix = create_associated_token_account(
                    &self.main_keypair.pubkey(), // fee payer
                    &self.main_keypair.pubkey(), // wallet
                    mint,
                );
                instructions.push(init_ix);
            }
        }

        // Transfer tokens from the deposit wallet to the main wallet
        let transfer_ix = spl_token::instruction::transfer_checked(
            &spl_token::id(),
            &temp_tok_pk,
            mint,
            &main_tok_pk,
            &keypair.pubkey(),
            &[],
            amount,
            decimals as u8,
        )?;
        instructions.push(transfer_ix);

        // Close the account and reap the rent if there's no more tokens on it.
        let (tok_balance, _) = get_account_token_balance(rpc, &temp_tok_pk, mint)?;
        if tok_balance - amount == 0 {
            debug!(target: "SOL BRIDGE", "Adding account close instruction because resulting balance is 0");
            let close_ix = spl_token::instruction::close_account(
                &spl_token::id(),
                &temp_tok_pk,
                &self.main_keypair.pubkey(),
                &keypair.pubkey(),
                &[],
            )?;
            instructions.push(close_ix);
        }

        let tx = Transaction::new_with_payer(&instructions, Some(&self.main_keypair.pubkey()));
        let signature = sign_and_send_transaction(rpc, tx, vec![&self.main_keypair, keypair])?;

        debug!(target: "SOL BRIDGE", "Sent tokens to main wallet: {}", signature);

        Ok(signature)
    }

    fn send_sol_to_main_wallet(
        self: Arc<Self>,
        rpc: &RpcClient,
        amount: u64,
        keypair: &Keypair,
    ) -> SolResult<Signature> {
        debug!(target: "SOL BRIDGE", "Sending {} SOL to main wallet", lamports_to_sol(amount));

        let ix =
            system_instruction::transfer(&keypair.pubkey(), &self.main_keypair.pubkey(), amount);
        let tx = Transaction::new_with_payer(&[ix], Some(&self.main_keypair.pubkey()));
        let signature = sign_and_send_transaction(rpc, tx, vec![&self.main_keypair, keypair])?;

        debug!(target: "SOL BRIDGE", "Sent {} SOL to main wallet: {}", lamports_to_sol(amount), signature);
        Ok(signature)
    }

    fn check_mint_address(&self, mint_address: Option<String>) -> SolResult<Option<Pubkey>> {
        if let Some(mint_addr) = mint_address {
            let pubkey = match Pubkey::from_str(&mint_addr) {
                Ok(v) => v,
                Err(e) => return Err(SolFailed::BadSolAddress(e.to_string())),
            };

            let rpc = RpcClient::new(self.rpc_server.to_string());

            if !account_is_initialized_mint(&rpc, &pubkey).0 {
                return Err(SolFailed::MintIsNotValid(mint_addr))
            }

            Ok(Some(pubkey))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl NetworkClient for SolClient {
    async fn subscribe(
        self: Arc<Self>,
        drk_pub_key: PublicKey,
        mint_address: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<TokenSubscribtion> {
        let keypair = SolKeypair(Keypair::new());

        let public_key = keypair.0.pubkey().to_string();
        let private_key = serialize(&keypair);

        let mint = self.check_mint_address(mint_address)?;

        let rpc = RpcClient::new(self.rpc_server.to_string());

        if !self.check_main_account_balance(&rpc)? {
            warn!(target: "SOL BRIDGE", "Main account has no enough funds");
            return Err(Error::from(SolFailed::MainAccountNotEnoughValue))
        }

        executor
            .spawn(async move {
                let result = self.handle_subscribe_request(keypair.0, drk_pub_key, mint).await;
                if let Err(e) = result {
                    error!(target: "SOL BRIDGE SUBSCRIPTION","{}", e.to_string());
                }
            })
            .detach();

        Ok(TokenSubscribtion { private_key, public_key })
    }

    // in solana case private key it's the same as keypair
    async fn subscribe_with_keypair(
        self: Arc<Self>,
        private_key: Vec<u8>,
        _public_key: Vec<u8>,
        drk_pub_key: PublicKey,
        mint_address: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<String> {
        let keypair: Keypair = deserialize::<SolKeypair>(&private_key)?.0;

        let public_key = keypair.pubkey().to_string();

        let mint = self.check_mint_address(mint_address)?;

        let rpc = RpcClient::new(self.rpc_server.to_string());

        if !self.check_main_account_balance(&rpc)? {
            return Err(Error::from(SolFailed::MainAccountNotEnoughValue))
        }

        executor
            .spawn(async move {
                let result = self.handle_subscribe_request(keypair, drk_pub_key, mint).await;
                if let Err(e) = result {
                    error!(target: "SOL BRIDGE SUBSCRIPTION","{}", e.to_string());
                }
            })
            .detach();

        Ok(public_key)
    }

    async fn get_notifier(self: Arc<Self>) -> Result<async_channel::Receiver<TokenNotification>> {
        Ok(self.notify_channel.1.clone())
    }

    async fn send(
        self: Arc<Self>,
        address: Vec<u8>,
        mint: Option<String>,
        amount: u64,
    ) -> Result<()> {
        debug!(target: "SOL BRIDGE", "start sending {} sol", lamports_to_sol(amount) );

        let rpc = RpcClient::new(self.rpc_server.to_string());
        let address: Pubkey = deserialize::<SolPubkey>(&address)?.0;

        let mut decimals = 9;

        if mint.is_some() {
            let mint_address: Option<Pubkey> = self.check_mint_address(mint)?;
            if let Some(mint_addr) = mint_address {
                let tkn = rpc.get_token_supply(&mint_addr).map_err(SolFailed::from)?;
                decimals = tkn.decimals;
            };
        }

        // reverse truncate
        let amount = truncate(amount, decimals as u16, 8)?;

        let instruction =
            system_instruction::transfer(&self.main_keypair.pubkey(), &address, amount);

        let mut tx = Transaction::new_with_payer(&[instruction], Some(&self.main_keypair.pubkey()));
        let bhq = BlockhashQuery::default();
        match bhq.get_blockhash(&rpc, rpc.commitment()) {
            Err(_) => panic!("Couldn't connect to RPC"),
            Ok(v) => tx.sign(&[&self.main_keypair], v),
        }

        let _signature = rpc.send_and_confirm_transaction(&tx).map_err(SolFailed::from)?;

        Ok(())
    }
}

/// Gets account token balance for given mint.
/// Returns: (amount, decimals)
pub fn get_account_token_balance(
    rpc: &RpcClient,
    address: &Pubkey,
    mint: &Pubkey,
) -> SolResult<(u64, u64)> {
    let mint_account = rpc.get_account(mint)?;
    let token_account = rpc.get_account(address)?;
    let mint_data = spl_token::state::Mint::unpack_from_slice(&mint_account.data)?;
    let token_data = spl_token::state::Account::unpack_from_slice(&token_account.data)?;

    Ok((token_data.amount, mint_data.decimals as u64))
}

/// Check if given account is a valid token mint
pub fn account_is_initialized_mint(rpc: &RpcClient, mint: &Pubkey) -> (bool, u64) {
    match rpc.get_token_supply(mint) {
        Ok(v) => (true, v.decimals as u64),
        Err(_) => (false, 0),
    }
}

pub fn sign_and_send_transaction(
    rpc: &RpcClient,
    mut tx: Transaction,
    signers: Vec<&Keypair>,
) -> SolResult<Signature> {
    let bhq = BlockhashQuery::default();
    match bhq.get_blockhash(rpc, rpc.commitment()) {
        Err(_) => return Err(SolFailed::RpcError("Couldn't connect to RPC".into())),
        Ok(v) => tx.sign(&signers, v),
    }

    match rpc.send_and_confirm_transaction(&tx) {
        Ok(s) => Ok(s),
        Err(_) => Err(SolFailed::RpcError("Failed to send transaction".into())),
    }
}

impl Encodable for SolKeypair {
    fn encode<S: std::io::Write>(&self, s: S) -> darkfi::Result<usize> {
        let key: Vec<u8> = self.0.to_bytes().to_vec();
        let len = key.encode(s)?;
        Ok(len)
    }
}

impl Decodable for SolKeypair {
    fn decode<D: std::io::Read>(mut d: D) -> darkfi::Result<Self> {
        let key: Vec<u8> = Decodable::decode(&mut d)?;
        let key = Keypair::from_bytes(key.as_slice())
            .map_err(|_| darkfi::Error::DecodeError("SOL BRIDGE: load keypair from slice"))?;
        Ok(SolKeypair(key))
    }
}

impl Encodable for SolPubkey {
    fn encode<S: std::io::Write>(&self, s: S) -> darkfi::Result<usize> {
        let key = self.0.to_string();
        let len = key.encode(s)?;
        Ok(len)
    }
}

impl Decodable for SolPubkey {
    fn decode<D: std::io::Read>(mut d: D) -> darkfi::Result<Self> {
        let key: String = Decodable::decode(&mut d)?;
        let key = Pubkey::try_from(key.as_str())
            .map_err(|_| darkfi::Error::DecodeError("SOL BRIDGE: load public key from slice"))?;
        Ok(SolPubkey(key))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SolFailed {
    #[error("There is no enough value `{0}`")]
    NotEnoughValue(u64),
    #[error("Main Account Has no enough value")]
    MainAccountNotEnoughValue,
    #[error("Bad Sol Address: `{0}`")]
    BadSolAddress(String),
    #[error("Decode and decode keys error: `{0}`")]
    DecodeAndEncodeError(String),
    #[error(transparent)]
    WebSocketError(#[from] tungstenite::Error),
    #[error("RpcError: `{0}`")]
    RpcError(String),
    #[error(transparent)]
    SolClientError(#[from] solana_client::client_error::ClientError),
    #[error("Received Notification Error: `{0}`")]
    Notification(String),
    #[error(transparent)]
    ProgramError(#[from] solana_sdk::program_error::ProgramError),
    #[error("Given mint is not valid: `{0}`")]
    MintIsNotValid(String),
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
    #[error(transparent)]
    ParseError(#[from] solana_sdk::pubkey::ParsePubkeyError),
    #[error("Signature Error: `{0}`")]
    Signature(String),
    #[error(transparent)]
    Darkfi(#[from] darkfi::error::Error),
}

impl From<SolFailed> for Error {
    fn from(error: SolFailed) -> Self {
        Error::CashierError(error.to_string())
    }
}

pub type SolResult<T> = std::result::Result<T, SolFailed>;
