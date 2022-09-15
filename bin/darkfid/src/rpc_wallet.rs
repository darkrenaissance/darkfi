use fxhash::FxHashMap;
use incrementalmerkletree::Tree;
use log::error;
use pasta_curves::group::ff::PrimeField;
use serde_json::{json, Value};

use darkfi::{
    crypto::{
        address::Address,
        keypair::{Keypair, PublicKey, SecretKey},
        token_id,
    },
    node::State,
    rpc::jsonrpc::{
        ErrorCode::{InternalError, InvalidParams, ParseError},
        JsonError, JsonResponse, JsonResult,
    },
    util::serial::{deserialize, serialize},
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
            Ok(a) => JsonResponse::new(json!(a.to_string()), id).into(),
            Err(e) => {
                error!("Failed creating keypair: {}", e);
                server_error(RpcError::Keygen, id)
            }
        }
    }

    // RPCAPI:
    // Fetches public keys by given indexes from the wallet and returns it in an
    // encoded format. `-1` is supported to fetch all available keys.
    // --> {"jsonrpc": "2.0", "method": "wallet.get_addrs", "params": [1, 2], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["foo", "bar"], "id": 1}
    pub async fn get_addrs(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
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

        JsonResponse::new(json!(ret), id).into()
    }

    // RPCAPI:
    // Exports the given keypair index.
    // Returns the encoded secret key upon success.
    // --> {"jsonrpc": "2.0", "method": "wallet.export_keypair", "params": [0], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "foobar", "id": 1}
    pub async fn export_keypair(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let keypairs = match self.client.get_keypairs().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching keypairs: {}", e);
                return server_error(RpcError::KeypairFetch, id)
            }
        };

        if let Some(kp) = keypairs.get(params[0].as_u64().unwrap() as usize) {
            return JsonResponse::new(json!(kp.secret.to_bytes()), id).into()
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
            return JsonError::new(InvalidParams, None, id).into()
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
                return JsonError::new(InternalError, None, id).into()
            }
        };

        JsonResponse::new(json!(address), id).into()
    }

    // RPCAPI:
    // Sets the default wallet address to the given index.
    // Returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "wallet.set_default_address", "params": [2], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn set_default_address(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return JsonError::new(InvalidParams, None, id).into()
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
                return JsonError::new(InternalError, None, id).into()
            }
        };

        JsonResponse::new(json!(true), id).into()
    }

    // RPCAPI:
    // Queries the wallet for known tokens with active balances.
    // Returns a map of balances, indexed by the token ID.
    // --> {"jsonrpc": "2.0", "method": "wallet.get_balances", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [{"1Foobar...": 100}, {...}]", "id": 1}
    pub async fn get_balances(&self, id: Value, _params: &[Value]) -> JsonResult {
        let balances = match self.client.get_balances().await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching balances from wallet: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        // k: token_id, v: [amount]
        let mut ret: FxHashMap<String, u64> = FxHashMap::default();

        for balance in balances.list {
            let token_id = bs58::encode(balance.token_id.to_repr()).into_string();
            let mut amount = balance.value;

            if let Some(prev) = ret.get(&token_id) {
                amount += prev;
            }

            ret.insert(token_id, amount);
        }

        JsonResponse::new(json!(ret), id).into()
    }

    // RPCAPI:
    // Queries the wallet for a coin containing given parameters (value, token_id, unspent),
    // and returns the entire row with the coin's data:
    //
    // --> {"jsonrpc": "2.0", "method": "wallet.get_coins_valtok", "params": [1234, "F00b4r...", true], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["coin", "data", ...], "id": 1}
    pub async fn get_coins_valtok(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 3 ||
            !params[0].is_u64() ||
            !params[1].is_string() ||
            !params[2].is_boolean()
        {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let value = params[0].as_u64().unwrap();
        let unspent = params[2].as_bool().unwrap();
        let token_id = match token_id::parse_b58(params[1].as_str().unwrap()) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed parsing token_id from base58 string: {}", e);
                return JsonError::new(ParseError, None, id).into()
            }
        };

        let coins = match self.client.get_coins_valtok(value, token_id, unspent).await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching coins by valtok from wallet: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        let ret: Vec<String> =
            coins.iter().map(|x| bs58::encode(serialize(x)).into_string()).collect();
        JsonResponse::new(json!(ret), id).into()
    }

    // RPCAPI:
    // Query the state merkle tree for the merkle path of a given leaf position.
    // --> {"jsonrpc": "2.0", "method": "wallet.get_merkle_path", "params": [3], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["f091uf1...", "081ff0h10w1h0...", ...], "id": 1}
    pub async fn get_merkle_path(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let leaf_pos: incrementalmerkletree::Position =
            ((params[0].as_u64().unwrap() as u64) as usize).into();

        let validator_state = self.validator_state.read().await;
        let state = validator_state.state_machine.lock().await;
        let root = state.tree.root(0).unwrap();
        let merkle_path = state.tree.authentication_path(leaf_pos, &root).unwrap();
        drop(state);
        drop(validator_state);

        let ret: Vec<String> =
            merkle_path.iter().map(|x| bs58::encode(serialize(x)).into_string()).collect();
        JsonResponse::new(json!(ret), id).into()
    }

    // RPCAPI:
    // Try to decrypt a given encrypted note with the secret keys
    // found in the wallet.
    // --> {"jsonrpc": "2.0", "method": "wallet.decrypt_note", params": [ciphertext], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "base58_encoded_plain_note", "id": 1}
    pub async fn decrypt_note(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let bytes = match bs58::decode(params[0].as_str().unwrap()).into_vec() {
            Ok(v) => v,
            Err(e) => {
                error!("decrypt_note(): Failed decoding base58 string: {}", e);
                return JsonError::new(ParseError, None, id).into()
            }
        };

        let enc_note = match deserialize(&bytes) {
            Ok(v) => v,
            Err(e) => {
                error!("decrypt_note(): Failed deserializing bytes into EncryptedNote: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        let keypairs = match self.client.get_keypairs().await {
            Ok(v) => v,
            Err(e) => {
                error!("decrypt_note(): Failed fetching keypairs: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        for kp in keypairs {
            if let Some(note) = State::try_decrypt_note(&enc_note, kp.secret) {
                let s = bs58::encode(&serialize(&note)).into_string();
                return JsonResponse::new(json!(s), id).into()
            }
        }

        server_error(RpcError::DecryptionFailed, id)
    }
}
