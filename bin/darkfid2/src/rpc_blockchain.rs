use log::{debug, error};
use serde_json::{json, Value};

use darkfi::rpc::{
    jsonrpc,
    jsonrpc::{
        ErrorCode::{InternalError, InvalidParams},
        JsonResult,
    },
};

use super::Darkfid;
use crate::{server_error, RpcError};

impl Darkfid {
    // RPCAPI:
    // Queries the blockchain database for a block in the given slot.
    // Returns a readable block upon success.
    // --> {"jsonrpc": "2.0", "method": "blockchain.get_slot", "params": [0], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {...}, "id": 1}
    pub async fn get_slot(&self, id: Value, params: &[Value]) -> JsonResult {
        if params.len() != 1 || !params[0].is_u64() {
            return jsonrpc::error(InvalidParams, None, id).into()
        }

        let blocks = match self
            .validator_state
            .read()
            .await
            .blockchain
            .get_blocks_by_slot(&[params[0].as_u64().unwrap()])
        {
            Ok(v) => v,
            Err(e) => {
                error!("Failed fetching block by slot: {}", e);
                return jsonrpc::error(InternalError, None, id).into()
            }
        };

        if blocks.is_empty() {
            return server_error(RpcError::UnknownSlot, id)
        }

        // TODO: Return block as JSON
        debug!("{:#?}", blocks[0]);
        jsonrpc::response(json!(true), id).into()
    }
}
