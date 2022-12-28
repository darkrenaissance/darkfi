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

use std::convert::TryInto;

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use hash_db::Hasher;
use keccak_hasher::KeccakHasher;
use lazy_static::lazy_static;
use log::{debug, error, info, trace};
use num_bigint::{BigUint, RandBigInt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

use super::bridge::{NetworkClient, TokenNotification, TokenSubscribtion};

use darkfi::{
    crypto::{keypair::PublicKey, token_id::generate_id2},
    rpc::{jsonrpc, jsonrpc::JsonResult},
    util::{
        parse::truncate,
        serial::{deserialize, serialize, Decodable, Encodable},
        sleep, NetworkName,
    },
    wallet::cashierdb::{CashierDb, TokenKey},
    Error, Result,
};

pub const ETH_NATIVE_TOKEN_ID: &str = "0x0000000000000000000000000000000000000000";

#[derive(Clone, Debug)]
pub struct Keypair {
    pub private_key: String,
    pub public_key: String,
}

impl Encodable for Keypair {
    fn encode<S: std::io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.private_key.encode(&mut s)?;
        len += self.public_key.encode(&mut s)?;
        Ok(len)
    }
}

impl Decodable for Keypair {
    fn decode<D: std::io::Read>(mut d: D) -> Result<Self> {
        Ok(Self { private_key: Decodable::decode(&mut d)?, public_key: Decodable::decode(&mut d)? })
    }
}

// An ERC-20 token transfer transaction's data is as follows:
//
// 1. The first 4 bytes of the keccak256 hash of "transfer(address,uint256)".
// 2. The address of the recipient, left-zero-padded to be 32 bytes.
// 3. The amount to be transferred: amount * 10^decimals

// This is the entire ERC20 ABI
lazy_static! {
    static ref ERC20_NAME_METHOD: [u8; 4] = {
        let method = b"name()";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
    static ref ERC20_APPROVE_METHOD: [u8; 4] = {
        let method = b"approve(address,uint256)";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
    static ref ERC20_TOTALSUPPLY_METHOD: [u8; 4] = {
        let method = b"totalSupply()";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
    static ref ERC20_TRANSFERFROM_METHOD: [u8; 4] = {
        let method = b"transferFrom(address,address,uint256)";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
    static ref ERC20_DECIMALS_METHOD: [u8; 4] = {
        let method = b"decimals()";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
    static ref ERC20_VERSION_METHOD: [u8; 4] = {
        let method = b"version()";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
    static ref ERC20_BALANCEOF_METHOD: [u8; 4] = {
        let method = b"balanceOf(address)";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
    static ref ERC20_SYMBOL_METHOD: [u8; 4] = {
        let method = b"symbol()";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
    static ref ERC20_TRANSFER_METHOD: [u8; 4] = {
        let method = b"transfer(address,uint256)";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
    static ref ERC20_APPROVEANDCALL_METHOD: [u8; 4] = {
        let method = b"approveAndCall(address,uint256,bytes)";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
    static ref ERC20_ALLOWANCE_METHOD: [u8; 4] = {
        let method = b"allowance(address,address)";
        KeccakHasher::hash(method)[0..4].try_into().expect("nope")
    };
}

pub fn erc20_transfer_data(recipient: &str, amount: BigUint) -> String {
    let rec = recipient.trim_start_matches("0x");
    let rec_padded = format!("{:0>64}", rec);

    let amnt_bytes = amount.to_bytes_be();
    let amnt_hex = hex::encode(amnt_bytes);
    let amnt_hex_padded = format!("{:0>64}", amnt_hex);

    format!("0x{}{}{}", hex::encode(*ERC20_TRANSFER_METHOD), rec_padded, amnt_hex_padded)
}

pub fn erc20_balanceof_data(account: &str) -> String {
    let acc = account.trim_start_matches("0x");
    let acc_padded = format!("{:0>64}", acc);

    format!("0x{}{}", hex::encode(*ERC20_BALANCEOF_METHOD), acc_padded)
}

fn to_eth_hex(val: BigUint) -> String {
    let bytes = val.to_bytes_be();
    let h = hex::encode(bytes);
    format!("0x{}", h.trim_start_matches('0'))
}

/// Generate a 256-bit ETH private key.
pub fn generate_privkey() -> String {
    let mut rng = rand::thread_rng();
    let token = rng.gen_bigint(256);
    let token_bytes = token.to_bytes_le().1;
    let key = KeccakHasher::hash(&token_bytes);
    hex::encode(key)
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EthTx {
    pub from: String,

    pub to: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gasPrice: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

impl EthTx {
    pub fn new(
        from: &str,
        to: &str,
        gas: Option<BigUint>,
        gas_price: Option<BigUint>,
        value: Option<BigUint>,
        data: Option<String>,
        nonce: Option<String>,
    ) -> Self {
        let gas_hex = gas.map(to_eth_hex);
        let gasprice_hex = gas_price.map(to_eth_hex);
        let value_hex = value.map(to_eth_hex);

        EthTx {
            from: from.to_string(),
            to: to.to_string(),
            gas: gas_hex,
            gasPrice: gasprice_hex,
            value: value_hex,
            data,
            nonce,
        }
    }
}

// JSON-RPC interface to Geth.
// https://eth.wiki/json-rpc/API
// https://geth.ethereum.org/docs/rpc/
//
// geth can be started with: $ geth --ropsten --syncmode light
// It should then show an Unix socket endpoint like so:
// INFO [10-25|19:47:32.845] IPC endpoint opened: url=/home/x/.ethereum/ropsten/geth.ipc
//
pub struct EthClient {
    pub main_keypair: Keypair,
    passphrase: String,
    socket_path: String,
    subscriptions: Arc<Mutex<Vec<String>>>,
    notify_channel:
        (async_channel::Sender<TokenNotification>, async_channel::Receiver<TokenNotification>),
}

impl EthClient {
    pub fn new(_network: &str, socket_path: &str, passphrase: &str) -> Self {
        let notify_channel = async_channel::unbounded();

        let subscriptions = Arc::new(Mutex::new(Vec::new()));

        let main_keypair = Keypair { public_key: "".into(), private_key: "".into() };

        Self {
            main_keypair,
            passphrase: passphrase.into(),
            socket_path: socket_path.into(),
            subscriptions,
            notify_channel,
        }
    }

    pub async fn setup_keypair(
        &mut self,
        cashier_wallet: Arc<CashierDb>,
        _keypair_path: &str,
    ) -> Result<()> {
        let main_keypair: Keypair;

        let main_keypairs = cashier_wallet.get_main_keys(&NetworkName::Ethereum).await?;

        if main_keypairs.is_empty() {
            let main_private_key = generate_privkey();
            let main_public_key =
                self.import_privkey(&main_private_key).await?.as_str().unwrap().to_string();

            cashier_wallet
                .put_main_keys(
                    &TokenKey {
                        secret_key: serialize(&main_private_key),
                        public_key: serialize(&main_public_key),
                    },
                    &NetworkName::Ethereum,
                )
                .await?;

            main_keypair = Keypair { private_key: main_private_key, public_key: main_public_key };
        } else {
            let last_keypair = &main_keypairs[main_keypairs.len() - 1];

            main_keypair = Keypair {
                private_key: deserialize(&last_keypair.secret_key)?,
                public_key: deserialize(&last_keypair.public_key)?,
            }
        }

        self.main_keypair = main_keypair;

        Ok(())
    }

    async fn send_eth_to_main_wallet(&self, acc: &str, amount: BigUint) -> Result<()> {
        info!(target: "ETH BRIDGE", "Sending eth to main wallet");

        let tx =
            EthTx::new(acc, &self.main_keypair.public_key, None, None, Some(amount), None, None);

        self.send_transaction(&tx, &self.passphrase).await?;

        Ok(())
    }

    async fn handle_subscribe_request(
        self: Arc<Self>,
        addr: String,
        drk_pub_key: PublicKey,
    ) -> Result<()> {
        if self.subscriptions.lock().await.contains(&addr) {
            return Ok(())
        }

        let decimals = 18;

        let prev_balance = self.get_current_balance(&addr, None).await?;

        let mut current_balance;

        let iter_interval = 1;
        let mut sub_iter = 0;

        loop {
            if sub_iter > 60 * 10 {
                // 10 minutes
                self.unsubscribe(&addr).await;
                return Err(EthFailed::Custom("Deposit for expired".to_string()).into())
            }

            sub_iter += iter_interval;
            sleep(iter_interval).await;

            current_balance = self.get_current_balance(&addr, None).await?;

            if current_balance != prev_balance {
                break
            }
        }

        let send_notification = self.notify_channel.0.clone();

        self.unsubscribe(&addr).await;

        if current_balance < prev_balance {
            return Err(
                EthFailed::Custom("New balance is less than previous balance".to_string()).into()
            )
        }

        let received_balance = current_balance - prev_balance;

        let received_balance_ui = received_balance.clone() / u64::pow(10, decimals as u32);

        send_notification
            .send(TokenNotification {
                network: NetworkName::Ethereum,
                token_id: generate_id2(ETH_NATIVE_TOKEN_ID, &NetworkName::Ethereum)?,
                drk_pub_key,
                // TODO FIX
                received_balance: received_balance.to_u64_digits()[0],
                decimals: decimals as u16,
            })
            .await
            .map_err(Error::from)?;

        self.send_eth_to_main_wallet(&addr, received_balance).await?;

        info!(target: "ETH BRIDGE", "Received {} eth", received_balance_ui );

        Ok(())
    }

    async fn unsubscribe(&self, pubkey: &str) {
        let mut subscriptions = self.subscriptions.lock().await;
        let index = subscriptions.iter().position(|p| p == pubkey);
        if let Some(ind) = index {
            trace!(target: "ETH BRIDGE", "Removing subscription from list");
            subscriptions.remove(ind);
        }
    }

    async fn request(&self, r: jsonrpc::JsonRequest) -> EthResult<Value> {
        debug!(target: "ETH RPC", "--> {}", serde_json::to_string(&r)?);
        let url = Url::parse(&format!("unix://{}", self.socket_path)).map_err(Error::from)?;
        let reply: JsonResult =
            match jsonrpc::send_request(&url, json!(r), None).await.map_err(EthFailed::from) {
                Ok(v) => v,
                Err(e) => return Err(e),
            };

        match reply {
            JsonResult::Resp(r) => {
                debug!(target: "ETH RPC", "<-- {}", serde_json::to_string(&r)?);
                Ok(r.result)
            }

            JsonResult::Err(e) => {
                debug!(target: "ETH RPC", "<-- {}", serde_json::to_string(&e)?);
                Err(EthFailed::RpcError(e.error.message.to_string()))
            }

            JsonResult::Notif(n) => {
                debug!(target: "ETH RPC", "<-- {}", serde_json::to_string(&n)?);
                Err(EthFailed::RpcError("Unexpected reply".to_string()))
            }
        }
    }

    pub async fn import_privkey(&self, key: &str) -> EthResult<Value> {
        let req = jsonrpc::request(json!("personal_importRawKey"), json!([key, self.passphrase]));
        Ok(self.request(req).await?)
    }

    /*
    pub async fn estimate_gas(&self, tx: &EthTx) -> Result<Value> {
    let req = jsonrpc::request(json!("eth_estimateGas"), json!([tx]));
    Ok(self.request(req).await?)
    }
    */

    pub async fn block_number(&self) -> EthResult<Value> {
        let req = jsonrpc::request(json!("eth_blockNumber"), json!([]));
        Ok(self.request(req).await?)
    }

    pub async fn get_eth_balance(&self, acc: &str, block: &str) -> EthResult<Value> {
        let req = jsonrpc::request(json!("eth_getBalance"), json!([acc, block]));
        Ok(self.request(req).await?)
    }

    pub async fn get_erc20_balance(&self, acc: &str, mint: &str) -> EthResult<Value> {
        let tx = EthTx::new(acc, mint, None, None, None, Some(erc20_balanceof_data(acc)), None);
        let req = jsonrpc::request(json!("eth_call"), json!([tx, "latest"]));
        Ok(self.request(req).await?)
    }

    pub async fn get_current_balance(&self, acc: &str, _mint: Option<&str>) -> EthResult<BigUint> {
        // Latest known block, used to calculate present balance.
        let block = self.block_number().await?;
        let block = block.as_str().unwrap();

        // Native ETH balance
        let hexbalance = self.get_eth_balance(acc, block).await?;
        let hexbalance = hexbalance.as_str().unwrap().trim_start_matches("0x");
        let balance = BigUint::parse_bytes(hexbalance.as_bytes(), 16).unwrap();

        Ok(balance)
    }

    pub async fn send_transaction(&self, tx: &EthTx, passphrase: &str) -> EthResult<Value> {
        let req = jsonrpc::request(json!("personal_sendTransaction"), json!([tx, passphrase]));
        Ok(self.request(req).await?)
    }
}

#[async_trait]
impl NetworkClient for EthClient {
    async fn subscribe(
        self: Arc<Self>,
        drk_pub_key: PublicKey,
        _mint_address: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<TokenSubscribtion> {
        let private_key = generate_privkey();

        let addr = self.import_privkey(&private_key).await?;

        let address: String = if addr.as_str().is_some() {
            addr.as_str().unwrap().to_string()
        } else {
            return Err(Error::from(EthFailed::ImportPrivateError))
        };

        let addr_cloned = address.clone();
        executor
            .spawn(async move {
                let result = self.handle_subscribe_request(addr_cloned, drk_pub_key).await;
                if let Err(e) = result {
                    error!(target: "ETH BRIDGE SUBSCRIPTION","{}", e.to_string());
                }
            })
            .detach();

        let private_key: Vec<u8> = serialize(&private_key);

        Ok(TokenSubscribtion { private_key, public_key: address })
    }

    async fn subscribe_with_keypair(
        self: Arc<Self>,
        _private_key: Vec<u8>,
        public_key: Vec<u8>,
        drk_pub_key: PublicKey,
        _mint_address: Option<String>,
        executor: Arc<Executor<'_>>,
    ) -> Result<String> {
        let public_key: String = deserialize(&public_key)?;

        let address = public_key.clone();
        executor
            .spawn(async move {
                let result = self.handle_subscribe_request(address, drk_pub_key).await;
                if let Err(e) = result {
                    error!(target: "ETH BRIDGE SUBSCRIPTION","{}", e.to_string());
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
        _mint: Option<String>,
        amount: u64,
    ) -> Result<()> {
        // Recipient address
        let dest: String = deserialize(&address)?;

        let decimals = 18;

        // reverse truncate
        let amount = truncate(amount, decimals as u16, 8)?;

        let tx = EthTx::new(
            &self.main_keypair.public_key,
            &dest,
            None,
            None,
            Some(BigUint::from(amount)),
            None,
            None,
        );

        self.send_transaction(&tx, &self.passphrase).await?;

        Ok(())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum EthFailed {
    #[error("There is no enough value {0}")]
    NotEnoughValue(u64),
    #[error("Main Account Has no enough value")]
    MainAccountNotEnoughValue,
    #[error("Bad Eth Address: {0}")]
    BadEthAddress(String),
    #[error("Decode and decode keys error: {0}")]
    DecodeAndEncodeError(String),
    #[error("Rpc Error: {0}")]
    RpcError(String),
    #[error("Eth client error: {0}")]
    EthClientError(String),
    #[error("Given mint is not valid: {0}")]
    MintIsNotValid(String),
    #[error("JsonError: {0}")]
    JsonError(String),
    #[error("Parse Error: {0}")]
    ParseError(String),
    #[error("Unable to derive address from private key")]
    ImportPrivateError,
    #[error("{0}")]
    Custom(String),
}

impl From<darkfi::Error> for EthFailed {
    fn from(err: darkfi::Error) -> EthFailed {
        EthFailed::EthClientError(err.to_string())
    }
}
impl From<serde_json::Error> for EthFailed {
    fn from(err: serde_json::Error) -> EthFailed {
        EthFailed::JsonError(err.to_string())
    }
}

impl From<EthFailed> for Error {
    fn from(error: EthFailed) -> Self {
        Error::CashierError(error.to_string())
    }
}

pub type EthResult<T> = std::result::Result<T, EthFailed>;

#[allow(unused_imports)]
mod tests {
    use super::*;
    use num_bigint::ToBigUint;
    use std::str::FromStr;

    #[test]
    fn test_erc20_transfer_data() {
        let recipient = "0x5b7b3b499fb69c40c365343cb0dc842fe8c23887";
        let amnt = BigUint::from_str("34765403556934000640").unwrap();

        assert_eq!(erc20_transfer_data(recipient, amnt), "0xa9059cbb0000000000000000000000005b7b3b499fb69c40c365343cb0dc842fe8c23887000000000000000000000000000000000000000000000001e27786570c272000");
    }
}
