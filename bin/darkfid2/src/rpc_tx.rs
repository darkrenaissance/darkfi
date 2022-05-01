use std::str::FromStr;

use log::{error, warn};
use serde_json::{json, Value};

use darkfi::{
    consensus::Tx,
    crypto::{address::Address, keypair::PublicKey, token_id::generate_id},
    rpc::{
        jsonrpc,
        jsonrpc::{
            ErrorCode::{InternalError, InvalidAddressParam, InvalidAmountParam, InvalidParams},
            JsonResult,
        },
    },
    util::{decode_base10, serial::serialize, NetworkName},
};

use super::Darkfid;
use crate::{server_error, RpcError};

impl Darkfid {
    // RPCAPI:
    // Transfer a given amount of some token to the given address.
    // Returns a transaction ID upon success.
    // --> {"jsonrpc": "2.0", "method": "tx.transfer", "params": ["darkfi" "gdrk", "1DarkFi...", 12.0], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID...", "id": 1}
    pub async fn transfer(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 4 ||
            !params[0].is_string() ||
            !params[1].is_string() ||
            !params[2].is_string() ||
            !params[3].is_f64()
        {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let network = params[0].as_str().unwrap();
        let token = params[1].as_str().unwrap();
        let address = params[2].as_str().unwrap();
        let amount = params[3].as_f64().unwrap();

        if !(*self.synced.lock().await) {
            error!("transfer(): Blockchain is not yet synced");
            return server_error(RpcError::NotYetSynced, id)
        }

        let address = match Address::from_str(address) {
            Ok(v) => v,
            Err(e) => {
                error!("transfer(): Failed parsing address from string: {}", e);
                return jsonrpc::error(InvalidAddressParam, None, id).into()
            }
        };

        let pubkey = match PublicKey::try_from(address) {
            Ok(v) => v,
            Err(e) => {
                error!("transfer(): Failed parsing PublicKey from Address: {}", e);
                return server_error(RpcError::ParseError, id)
            }
        };

        let amount = amount.to_string();
        let amount = match decode_base10(&amount, 8, true) {
            Ok(v) => v,
            Err(e) => {
                error!("transfer(): Failed parsing amount from string: {}", e);
                return jsonrpc::error(InvalidAmountParam, None, id).into()
            }
        };
        let amount: u64 = match amount.try_into() {
            Ok(v) => v,
            Err(e) => {
                error!("transfer(): Failed converting biguint to u64: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        };

        let network = match NetworkName::from_str(network) {
            Ok(v) => v,
            Err(e) => {
                error!("transfer(): Failed parsing NetworkName: {}", e);
                return server_error(RpcError::NetworkNameError, id)
            }
        };

        let token_id =
            if let Some(tok) = self.client.tokenlist.by_net[&network].get(token.to_uppercase()) {
                tok.drk_address
            } else {
                match generate_id(&network, token) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("transfer(): Failed generate_id(): {}", e);
                        return jsonrpc::error(InternalError, None, id).into()
                    }
                }
            };

        let tx = match self
            .client
            .build_transaction(
                pubkey,
                amount,
                token_id,
                false,
                self.validator_state.read().await.state_machine.clone(),
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("transfer(): Failed building transaction: {}", e);
                return server_error(RpcError::TxBuildFail, id)
            }
        };

        if let Some(sync_p2p) = &self.sync_p2p {
            match sync_p2p.broadcast(Tx(tx.clone())).await {
                Ok(()) => {}
                Err(e) => {
                    error!("transfer(): Failed broadcasting transaction: {}", e);
                    return server_error(RpcError::TxBroadcastFail, id)
                }
            }
        } else {
            warn!("No sync P2P network, not broadcasting transaction.");
        }

        let tx_hash = blake3::hash(&serialize(&tx)).to_hex().as_str().to_string();
        jsonrpc::response(json!(tx_hash), id).into()
    }
}
