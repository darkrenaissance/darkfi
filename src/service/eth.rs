use std::convert::TryInto;

use hash_db::Hasher;
use keccak_hasher::KeccakHasher;
use lazy_static::lazy_static;
use log::debug;
use num_bigint::{BigUint, RandBigInt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{rpc::jsonrpc, rpc::jsonrpc::JsonResult, Error, Result};

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

    format!(
        "0x{}{}{}",
        hex::encode(*ERC20_TRANSFER_METHOD),
        rec_padded,
        amnt_hex_padded
    )
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
        let gas_hex = match gas {
            Some(v) => Some(to_eth_hex(v)),
            None => None,
        };

        let gasprice_hex = match gas_price {
            Some(v) => Some(to_eth_hex(v)),
            None => None,
        };

        let value_hex = match value {
            Some(v) => Some(to_eth_hex(v)),
            None => None,
        };

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
    socket_path: String,
}

impl EthClient {
    pub fn new(socket_path: String) -> Self {
        Self { socket_path }
    }

    async fn request(&self, r: jsonrpc::JsonRequest) -> Result<Value> {
        debug!(target: "ETH RPC", "--> {}", serde_json::to_string(&r)?);
        let reply: JsonResult = match jsonrpc::send_unix_request(&self.socket_path, json!(r)).await
        {
            Ok(v) => v,
            Err(e) => return Err(e),
        };

        match reply {
            JsonResult::Resp(r) => {
                debug!(target: "ETH RPC", "<-- {}", serde_json::to_string(&r)?);
                return Ok(r.result);
            }

            JsonResult::Err(e) => {
                debug!(target: "ETH RPC", "<-- {}", serde_json::to_string(&e)?);
                return Err(Error::JsonRpcError(e.error.message.to_string()));
            }

            JsonResult::Notif(n) => {
                debug!(target: "ETH RPC", "<-- {}", serde_json::to_string(&n)?);
                return Err(Error::JsonRpcError("Unexpected reply".to_string()));
            }
        }
    }

    pub async fn import_privkey(&self, key: &str, passphrase: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("personal_importRawKey"), json!([key, passphrase]));
        Ok(self.request(req).await?)
    }

    /*
    pub async fn estimate_gas(&self, tx: &EthTx) -> Result<Value> {
        let req = jsonrpc::request(json!("eth_estimateGas"), json!([tx]));
        Ok(self.request(req).await?)
    }
    */

    pub async fn block_number(&self) -> Result<Value> {
        let req = jsonrpc::request(json!("eth_blockNumber"), json!([]));
        Ok(self.request(req).await?)
    }

    pub async fn get_eth_balance(&self, acc: &str, block: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("eth_getBalance"), json!([acc, block]));
        Ok(self.request(req).await?)
    }

    pub async fn get_erc20_balance(&self, acc: &str, mint: &str) -> Result<Value> {
        let tx = EthTx::new(
            acc,
            mint,
            None,
            None,
            None,
            Some(erc20_balanceof_data(acc)),
            None,
        );
        let req = jsonrpc::request(json!("eth_call"), json!([tx, "latest"]));
        Ok(self.request(req).await?)
    }

    pub async fn send_transaction(&self, tx: &EthTx, passphrase: &str) -> Result<Value> {
        let req = jsonrpc::request(json!("personal_sendTransaction"), json!([tx, passphrase]));
        Ok(self.request(req).await?)
    }
}

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
