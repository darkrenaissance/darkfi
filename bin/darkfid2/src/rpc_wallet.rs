use fxhash::FxHashMap;
use log::{error, warn};
use num_bigint::BigUint;
use pasta_curves::group::ff::PrimeField;
use serde_json::{json, Value};

use darkfi::{
    crypto::{
        address::Address,
        keypair::{Keypair, PublicKey, SecretKey},
    },
    rpc::{
        jsonrpc,
        jsonrpc::{
            ErrorCode::{InternalError, InvalidParams},
            JsonResult,
        },
    },
    util::{decode_base10, encode_base10, NetworkName},
};

use super::Darkfid;
use crate::{server_error, RpcError};

impl Darkfid {
    // RPCAPI:
    // Attempts to generate a new keypair and returns its address upon success.
    // --> {"jsonrpc": "2.0", "method": "wallet.keygen", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "1DarkFi...", "id": 1}
    pub async fn keygen(&self, id: Value, _params: &[Value]) -> JsonResult {
        match self.client.keygen().await {
            Ok(a) => jsonrpc::response(json!(a.to_string()), id).into(),
            Err(e) => {
                error!("Failed creating keypair: {}", e);
                server_error(RpcError::Keygen, id)
            }
        }
    }

    // RPCAPI:
    // Fetches public keys by given indexes from the wallet and returns it in an
    // encoded format. `-1` is supported to fetch all available keys.
    // --> {"jsonrpc": "2.0", "method": "wallet.get_key", "params": [1, 2], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["foo", "bar"], "id": 1}
    pub async fn get_key(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.is_empty() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let mut fetch_all = false;
        for i in params {
            if !i.is_i64() {
                return server_error(RpcError::Nan, id)
            }

            if i.as_i64() == Some(-1) {
                fetch_all = true;
                break
            }

            if i.as_i64() < Some(-1) {
                return server_error(RpcError::LessThanNegOne, id)
            }
        }

        let keypairs = match self.client.get_keypairs().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching keypairs: {}", e);
                return server_error(RpcError::KeypairFetch, id)
            }
        };

        let mut ret = vec![];

        if fetch_all {
            ret = keypairs.iter().map(|x| Some(Address::from(x.public).to_string())).collect()
        } else {
            for i in params {
                // This cast is safe on 64bit since we've already sorted out
                // all negative cases above.
                let idx = i.as_i64().unwrap() as usize;
                if let Some(kp) = keypairs.get(idx) {
                    ret.push(Some(Address::from(kp.public).to_string()));
                } else {
                    ret.push(None)
                }
            }
        }

        jsonrpc::response(json!(ret), id).into()
    }

    // RPCAPI:
    // Exports the given keypair index.
    // Returns the encoded secret key upon success.
    // --> {"jsonrpc": "2.0", "method": "wallet.export_keypair", "params": [0], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "foobar", "id": 1}
    pub async fn export_keypair(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let keypairs = match self.client.get_keypairs().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching keypairs: {}", e);
                return server_error(RpcError::KeypairFetch, id)
            }
        };

        if let Some(kp) = keypairs.get(params[0].as_u64().unwrap() as usize) {
            return jsonrpc::response(json!(kp.secret.to_bytes()), id).into()
        }

        server_error(RpcError::KeypairNotFound, id)
    }

    // RPCAPI:
    // Imports a given secret key into the wallet as a keypair.
    // Returns the public counterpart as the result upon success.
    // --> {"jsonrpc": "2.0", "method": "wallet.import_keypair", "params": ["foobar"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "pubfoobar", "id": 1}
    pub async fn import_keypair(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let bytes: [u8; 32] = match serde_json::from_str(params[0].as_str().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing secret key from string: {}", e);
                return server_error(RpcError::InvalidKeypair, id)
            }
        };

        let secret = match SecretKey::from_bytes(bytes) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing secret key from string: {}", e);
                return server_error(RpcError::InvalidKeypair, id)
            }
        };

        let public = PublicKey::from_secret(secret);
        let keypair = Keypair { secret, public };
        let address = Address::from(public).to_string();

        match self.client.put_keypair(&keypair).await {
            Ok(()) => {}
            Err(e) => {
                error!("Failed inserting keypair into wallet: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        };

        jsonrpc::response(json!(address), id).into()
    }

    // RPCAPI:
    // Sets the default wallet address to the given index.
    // Returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "wallet.set_default_address", "params": [2], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn set_default_address(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let idx = params[0].as_u64().unwrap();

        let keypairs = match self.client.get_keypairs().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching keypairs: {}", e);
                return server_error(RpcError::KeypairFetch, id)
            }
        };

        if keypairs.len() as u64 != idx - 1 {
            return server_error(RpcError::KeypairNotFound, id)
        }

        let kp = keypairs[idx as usize];
        match self.client.set_default_keypair(&kp.public).await {
            Ok(()) => {}
            Err(e) => {
                error!("Failed setting default keypair: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        };

        jsonrpc::response(json!(true), id).into()
    }

    // RPCAPI:
    // Queries the wallet for known balances.
    // Returns a map of balances, indexed by `network`, and token ID.
    // --> {"jsonrpc": "2.0", "method": "wallet.get_balances", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [{"btc": [100, "Bitcoin"]}, {...}], "id": 1}
    pub async fn get_balances(&self, id: Value, _params: &[Value]) -> JsonResult {
        let balances = match self.client.get_balances().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching balances from wallet: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        };

        // k: ticker/drk_addr, v: (amount, network, net_addr, drk_addr)
        let mut ret: FxHashMap<String, (String, String, String, String)> = FxHashMap::default();

        for balance in balances.list {
            let drk_addr = bs58::encode(balance.token_id.to_repr()).into_string();
            let mut amount = BigUint::from(balance.value);

            let (net_name, net_addr) =
                if let Some((net, tok)) = self.tokenlist.by_addr.get(&drk_addr) {
                    (net, tok.net_address.clone())
                } else {
                    warn!("Could not find network name and token info for {}", drk_addr);
                    (&NetworkName::DarkFi, "unknown".to_string())
                };

            let mut ticker = None;
            for (k, v) in self.tokenlist.by_net[net_name].0.iter() {
                if v.net_address == net_addr {
                    ticker = Some(k.clone());
                    break
                }
            }

            if ticker.is_none() {
                ticker = Some(drk_addr.clone())
            }

            let ticker = ticker.unwrap();

            if let Some(prev) = ret.get(&ticker) {
                // TODO: We shouldn't be hardcoding everything to 8 decimals.
                let prev_amnt = match decode_base10(&prev.0, 8, false) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("Failed to decode_base10(): {}", e);
                        return jsonrpc::error(InternalError, None, id).into()
                    }
                };

                amount += prev_amnt;
            }

            let amount = encode_base10(amount, 8);
            ret.insert(ticker, (amount, net_name.to_string(), net_addr, drk_addr));
        }

        jsonrpc::response(json!(ret), id).into()
    }
}
