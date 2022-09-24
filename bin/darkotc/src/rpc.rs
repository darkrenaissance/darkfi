use std::{process::exit, str::FromStr};

use serde_json::json;

use darkfi::{
    crypto::{
        address::Address,
        coin::OwnCoin,
        merkle_node::MerkleNode,
        note::{EncryptedNote, Note},
    },
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    serial::{deserialize, serialize},
    Result,
};

/// The RPC object with functionality for connecting to darkfid.
pub struct Rpc {
    pub rpc_client: RpcClient,
}

impl Rpc {
    /// Fetch wallet balance of given token ID and return its u64 representation.
    pub async fn balance_of(&self, token_id: &str) -> Result<u64> {
        let req = JsonRequest::new("wallet.get_balances", json!([]));
        let rep = self.rpc_client.request(req).await?;

        if !rep.is_object() {
            eprintln!("Error: Invalid balance data received from darkfid RPC endpoint.");
            exit(1);
        }

        for i in rep.as_object().unwrap().keys() {
            if i == token_id {
                if let Some(balance) = rep[i].as_u64() {
                    return Ok(balance)
                }

                eprintln!("Error: Invalid balance data received from darkfid RPC endpoint.");
                exit(1);
            }
        }

        Ok(0)
    }

    /// Fetch default wallet address from the darkfid RPC endpoint.
    pub async fn wallet_address(&self) -> Result<Address> {
        let req = JsonRequest::new("wallet.get_addrs", json!([0_i64]));
        let rep = self.rpc_client.request(req).await?;

        if !rep.is_array() || !rep.as_array().unwrap()[0].is_string() {
            eprintln!("Error: Invalid wallet address received from darkfid RPC endpoint.");
            exit(1);
        }

        match Address::from_str(rep[0].as_str().unwrap()) {
            Ok(v) => Ok(v),
            Err(e) => {
                eprintln!(
                    "Error: Invalid wallet address received from darkfid RPC endpoint: {}",
                    e
                );
                exit(1)
            }
        }
    }

    /// Query wallet for unspent coins in wallet matching value and token_id.
    pub async fn get_coins_valtok(&self, value: u64, token_id: &str) -> Result<Vec<OwnCoin>> {
        let req = JsonRequest::new("wallet.get_coins_valtok", json!([value, token_id, true]));
        let rep = self.rpc_client.request(req).await?;

        if !rep.is_array() {
            eprintln!("Error: Invalid coin data received from darkfid RPC endpoint.");
            exit(1);
        }

        let rep = rep.as_array().unwrap();
        let mut ret = vec![];

        for i in rep {
            if !i.is_string() {
                eprintln!(
                    "Error: Invalid base58 data for OwnCoin received from darkfid RPC endpoint."
                );
                exit(1);
            }

            let data = match bs58::decode(i.as_str().unwrap()).into_vec() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Failed decoding base58 data for OwnCoin: {}", e);
                    exit(1);
                }
            };

            let oc = match deserialize(&data) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Failed deserializing OwnCoin: {}", e);
                    exit(1);
                }
            };

            ret.push(oc);
        }

        Ok(ret)
    }

    /// Fetch the merkle path for a given leaf position in the coin tree
    pub async fn get_merkle_path(&self, leaf_pos: usize) -> Result<Vec<MerkleNode>> {
        let req = JsonRequest::new("wallet.get_merkle_path", json!([leaf_pos as u64]));
        let rep = self.rpc_client.request(req).await?;

        if !rep.is_array() {
            eprintln!("Error: Invalid merkle path data received from darkfid RPC endpoint.");
            exit(1);
        }

        let rep = rep.as_array().unwrap();
        let mut ret = vec![];

        for i in rep {
            if !i.is_string() {
                eprintln!("Error: Invalid base58 data received for MerkleNode");
                exit(1);
            }

            let n = match bs58::decode(i.as_str().unwrap()).into_vec() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Failed decoding base58 for MerkleNode: {}", e);
                    exit(1);
                }
            };

            if n.len() != 32 {
                eprintln!("error: MerkleNode byte length is not 32");
                exit(1);
            }

            let n = MerkleNode::from_bytes(&n.try_into().unwrap());
            if n.is_some().unwrap_u8() == 0 {
                eprintln!("Error: Noncanonical bytes of MerkleNode");
                exit(1);
            }

            ret.push(n.unwrap());
        }

        Ok(ret)
    }

    /// Try to decrypt a given `EncryptedNote`
    pub async fn decrypt_note(&self, enc_note: &EncryptedNote) -> Result<Option<Note>> {
        let encoded = bs58::encode(&serialize(enc_note)).into_string();
        let req = JsonRequest::new("wallet.decrypt_note", json!([encoded]));
        let rep = self.rpc_client.oneshot_request(req).await?;

        if !rep.is_string() {
            eprintln!("Error: decrypt_note() RPC call returned invalid data");
            exit(1);
        }

        let decoded = match bs58::decode(rep.as_str().unwrap()).into_vec() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Error decoding base58 data received from RPC call: {}", e);
                exit(1);
            }
        };

        let note = match deserialize(&decoded) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Failed deserializing bytes into Note: {}", e);
                exit(1);
            }
        };

        Ok(Some(note))
    }
}
