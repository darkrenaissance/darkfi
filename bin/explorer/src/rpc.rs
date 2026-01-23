/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::collections::HashMap;

use darkfi::{
    blockchain::{header_store::PowData, BlockInfo},
    rpc::jsonrpc::{
        ErrorCode::{InternalError, InvalidParams},
        JsonError, JsonResponse, JsonResult,
    },
    tx::Transaction,
    util::encoding::base64,
};
use darkfi_serial::serialize_async;
use tinyjson::JsonValue;

use crate::{DifficultyIndex, Explorer};

struct ContractCallInfo {
    contract_id: String,
    contract_tag: Option<String>,
    func: String,
    size: u64,
}

impl ContractCallInfo {
    fn new(contract_id: String, func: String, size: u64) -> Self {
        let contract_tag = match contract_id.as_str() {
            "BZHKGQ26bzmBithTQYTJtjo2QdCqpkR9tjSBopT4yf4o" => Some("Money".to_string()),
            "Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj" => Some("DAO".to_string()),
            "EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN" => Some("Deployoor".to_string()),
            _ => None,
        };

        Self { contract_id, contract_tag, func, size }
    }

    fn to_json(&self) -> JsonValue {
        let tag = match &self.contract_tag {
            Some(t) => JsonValue::String(t.clone()),
            None => JsonValue::Null,
        };

        JsonValue::Object(HashMap::from([
            ("contract_id".to_string(), JsonValue::String(self.contract_id.clone())),
            ("contract_tag".to_string(), tag),
            ("func".to_string(), JsonValue::String(self.func.clone())),
            ("size".to_string(), JsonValue::Number(self.size as f64)),
        ]))
    }
}

struct TransactionInfo {
    hash: String,
    calls: Vec<ContractCallInfo>,
    fee: u64,
    size: u64,
}

impl TransactionInfo {
    async fn new(tx: &Transaction) -> Self {
        let mut calls = Vec::with_capacity(tx.calls.len());
        for call in &tx.calls {
            let call_data = serialize_async(&call.data).await;
            let func = if call_data.is_empty() {
                "0x00".to_string()
            } else {
                format!("0x{:02x}", call_data[0])
            };
            calls.push(ContractCallInfo::new(
                call.data.contract_id.to_string(),
                func,
                call_data.len() as u64,
            ));
        }

        Self {
            hash: tx.hash().to_string(),
            calls,
            fee: 0,
            size: serialize_async(tx).await.len() as u64,
        }
    }

    fn to_json(&self) -> JsonValue {
        let calls = self.calls.iter().map(|c| c.to_json()).collect();

        JsonValue::Object(HashMap::from([
            ("hash".to_string(), JsonValue::String(self.hash.clone())),
            ("calls".to_string(), JsonValue::Array(calls)),
            ("fee".to_string(), JsonValue::Number(self.fee as f64)),
            ("size".to_string(), JsonValue::Number(self.size as f64)),
        ]))
    }
}

/// Full transaction info for the get_tx RPC endpoint
struct ExplTxInfo {
    hash: String,
    from_block: u64,
    confirmations: u64,
    fee: u64,
    size: u64,
    n_calls: u64,
    calls: Vec<ContractCallInfo>,
    raw: String,
}

impl ExplTxInfo {
    async fn new(tx: &Transaction, block_height: u64, current_height: u64) -> Self {
        let mut calls = Vec::with_capacity(tx.calls.len());
        for call in &tx.calls {
            let call_data = serialize_async(&call.data).await;
            let func = if call_data.is_empty() {
                "0x00".to_string()
            } else {
                format!("0x{:02x}", call_data[0])
            };
            calls.push(ContractCallInfo::new(
                call.data.contract_id.to_string(),
                func,
                call_data.len() as u64,
            ));
        }

        let raw_bytes = serialize_async(tx).await;
        let confirmations =
            if current_height >= block_height { current_height - block_height + 1 } else { 0 };

        Self {
            hash: tx.hash().to_string(),
            from_block: block_height,
            confirmations,
            fee: 0,
            size: raw_bytes.len() as u64,
            n_calls: tx.calls.len() as u64,
            calls,
            raw: base64::encode(&raw_bytes),
        }
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::Object(HashMap::from([
            ("hash".to_string(), JsonValue::String(self.hash.clone())),
            ("from_block".to_string(), JsonValue::Number(self.from_block as f64)),
            ("confirmations".to_string(), JsonValue::Number(self.confirmations as f64)),
            ("fee".to_string(), JsonValue::Number(self.fee as f64)),
            ("size".to_string(), JsonValue::Number(self.size as f64)),
            ("n_calls".to_string(), JsonValue::Number(self.n_calls as f64)),
            (
                "calls".to_string(),
                JsonValue::Array(self.calls.iter().map(|c| c.to_json()).collect()),
            ),
            ("raw".to_string(), JsonValue::String(self.raw.clone())),
        ]))
    }
}

struct ExplBlockInfo {
    height: u64,
    hash: String,
    version: u8,
    previous_hash: String,
    nonce: u64,
    timestamp: u64,
    transactions_root: String,
    state_root: String,
    size: u64,
    difficulty: u64,
    cumulative: u64,
    powtype: String,
    txs: Vec<TransactionInfo>,
    coinbase: CoinbaseInfo,
}

impl ExplBlockInfo {
    async fn new(block: &BlockInfo, diff: &DifficultyIndex) -> Self {
        let powtype = match block.header.pow_data {
            PowData::DarkFi => "DarkFi".to_string(),
            PowData::Monero(_) => "Monero".to_string(),
        };

        let mut txs = Vec::with_capacity(block.txs.len());
        for tx in &block.txs {
            txs.push(TransactionInfo::new(tx).await);
        }

        let coinbase = CoinbaseInfo::new(&block.txs[block.txs.len() - 1]).await;

        Self {
            height: block.header.height as u64,
            hash: block.header.hash().to_string(),
            version: block.header.version,
            previous_hash: block.header.previous.to_string(),
            nonce: block.header.nonce as u64,
            timestamp: block.header.timestamp.inner(),
            transactions_root: block.header.transactions_root.to_string(),
            state_root: hex::encode(block.header.state_root),
            size: serialize_async(block).await.len() as u64,
            difficulty: diff.difficulty,
            cumulative: diff.cumulative,
            powtype,
            txs,
            coinbase,
        }
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::Object(HashMap::from([
            ("height".to_string(), JsonValue::Number(self.height as f64)),
            ("hash".to_string(), JsonValue::String(self.hash.clone())),
            ("version".to_string(), JsonValue::Number(self.version as f64)),
            ("previous_hash".to_string(), JsonValue::String(self.previous_hash.clone())),
            ("nonce".to_string(), JsonValue::Number(self.nonce as f64)),
            ("timestamp".to_string(), JsonValue::Number(self.timestamp as f64)),
            ("transactions_root".to_string(), JsonValue::String(self.transactions_root.clone())),
            ("state_root".to_string(), JsonValue::String(self.state_root.clone())),
            ("size".to_string(), JsonValue::Number(self.size as f64)),
            ("difficulty".to_string(), JsonValue::Number(self.difficulty as f64)),
            ("cumulative".to_string(), JsonValue::Number(self.cumulative as f64)),
            ("powtype".to_string(), JsonValue::String(self.powtype.clone())),
            ("txs".to_string(), JsonValue::Array(self.txs.iter().map(|t| t.to_json()).collect())),
            ("coinbase".to_string(), self.coinbase.to_json()),
        ]))
    }
}

struct CoinbaseInfo {
    hash: String,
    reward: u64,
    size: u64,
}

impl CoinbaseInfo {
    async fn new(tx: &Transaction) -> Self {
        Self {
            hash: tx.hash().to_string(),
            reward: 0,
            size: serialize_async(tx).await.len() as u64,
        }
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::Object(HashMap::from([
            ("hash".to_string(), JsonValue::String(self.hash.clone())),
            ("reward".to_string(), JsonValue::Number(self.reward as f64)),
            ("size".to_string(), JsonValue::Number(self.size as f64)),
        ]))
    }
}

impl Explorer {
    pub async fn rpc_current_difficulty(&self, id: u16, _params: JsonValue) -> JsonResult {
        // Get latest height
        let Ok(Some(height)) = self.get_height() else {
            return JsonError::new(InternalError, None, id).into()
        };

        let Ok(Some(difficulty)) = self.get_difficulty(height) else {
            return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(
            JsonValue::Array(vec![
                (difficulty.difficulty as f64).into(),
                (difficulty.cumulative as f64).into(),
            ]),
            id,
        )
        .into()
    }

    pub async fn rpc_current_height(&self, id: u16, _params: JsonValue) -> JsonResult {
        let Ok(Some(height)) = self.get_height() else {
            return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(JsonValue::Number(height as f64), id).into()
    }

    pub async fn rpc_latest_blocks(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_number() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let n_blocks = *params[0].get::<f64>().unwrap() as u64;

        let Ok(Some(height)) = self.get_height() else {
            return JsonError::new(InternalError, None, id).into()
        };

        // Calculate how many blocks we can actually return
        let start_height = height.saturating_sub(n_blocks.saturating_sub(1));
        let mut blocks = Vec::with_capacity((height - start_height + 1) as usize);

        for h in (start_height..=height).rev() {
            let Ok(Some((header, tx_count, size))) = self.get_block_summary(h).await else {
                return JsonError::new(InternalError, None, id).into()
            };

            let powtype = match header.pow_data {
                PowData::DarkFi => "DarkFi".to_string(),
                PowData::Monero(_) => "Monero".to_string(),
            };

            blocks.push(JsonValue::Object(HashMap::from([
                ("height".to_string(), JsonValue::Number(header.height as f64)),
                ("size".to_string(), JsonValue::Number(size as f64)),
                ("n_txs".to_string(), JsonValue::Number(tx_count as f64)),
                ("timestamp".to_string(), JsonValue::Number(header.timestamp.inner() as f64)),
                ("powtype".to_string(), JsonValue::String(powtype)),
                ("hash".to_string(), JsonValue::String(header.hash().to_string())),
            ])));
        }

        JsonResponse::new(JsonValue::Array(blocks), id).into()
    }

    pub async fn rpc_get_block(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let height = if params[0].is_string() {
            let Ok(hash) = hex::decode(params[0].get::<String>().unwrap()) else {
                return JsonError::new(InvalidParams, None, id).into()
            };

            let Ok(Some(height_bytes)) = self.header_indices.get(hash) else {
                return JsonError::new(InternalError, None, id).into()
            };

            let height_bytes = height_bytes.to_vec();
            u64::from_le_bytes(height_bytes.try_into().unwrap())
        } else if params[0].is_number() {
            *params[0].get::<f64>().unwrap() as u64
        } else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        let Ok(Some(block)) = self.get_block(height).await else {
            return JsonError::new(InternalError, None, id).into()
        };

        let Ok(Some(diff)) = self.get_difficulty(height) else {
            return JsonError::new(InternalError, None, id).into()
        };

        let info = ExplBlockInfo::new(&block, &diff).await;
        JsonResponse::new(info.to_json(), id).into()
    }

    pub async fn rpc_get_tx(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let tx_hash_str = params[0].get::<String>().unwrap();

        // Get current height for confirmations calculation
        let Ok(Some(current_height)) = self.get_height() else {
            return JsonError::new(InternalError, None, id).into()
        };

        // Get transaction by hash
        let Ok(Some((tx, block_height))) = self.get_tx_by_hash_str(tx_hash_str).await else {
            return JsonError::new(InternalError, None, id).into()
        };

        let info = ExplTxInfo::new(&tx, block_height, current_height).await;
        JsonResponse::new(info.to_json(), id).into()
    }

    /// Search for a block or transaction by hash.
    /// Returns `{"type": "block", "height": N}` or `{"type": "tx"}` depending on what was found.
    pub async fn rpc_search(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let query = params[0].get::<String>().unwrap();

        // Try to decode as hex
        let Ok(hash_bytes) = hex::decode(query) else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Try block hash first (serialized blake3 hash)
        if let Ok(Some(height_bytes)) = self.header_indices.get(&hash_bytes) {
            let height = u64::from_le_bytes(height_bytes.as_ref().try_into().unwrap_or([0u8; 8]));
            return JsonResponse::new(
                JsonValue::Object(HashMap::from([
                    ("type".to_string(), JsonValue::String("block".to_string())),
                    ("height".to_string(), JsonValue::Number(height as f64)),
                ])),
                id,
            )
            .into()
        }

        // Try transaction hash (32 bytes)
        if hash_bytes.len() == 32 {
            let mut tx_hash = [0u8; 32];
            tx_hash.copy_from_slice(&hash_bytes);
            if self.tx_indices.get(tx_hash).ok().flatten().is_some() {
                return JsonResponse::new(
                    JsonValue::Object(HashMap::from([(
                        "type".to_string(),
                        JsonValue::String("tx".to_string()),
                    )])),
                    id,
                )
                .into()
            }
        }

        // Not found
        JsonError::new(InternalError, Some("Not found".to_string()), id).into()
    }

    /// Calculate the current network hashrate.
    /// Hashrate = difficulty / average_block_time
    /// We use the last N blocks to smooth out variance.
    pub async fn rpc_get_hashrate(&self, id: u16, _params: JsonValue) -> JsonResult {
        const BLOCKS_TO_AVERAGE: u64 = 30;

        let Ok(Some(height)) = self.get_height() else {
            return JsonError::new(InternalError, None, id).into()
        };

        if height < 2 {
            return JsonResponse::new(JsonValue::Number(0.0), id).into()
        }

        let start_height = height.saturating_sub(BLOCKS_TO_AVERAGE);

        // Get timestamps from start and end blocks
        let Ok(Some(start_header)) = self.get_header(start_height).await else {
            return JsonError::new(InternalError, None, id).into()
        };
        let Ok(Some(end_header)) = self.get_header(height).await else {
            return JsonError::new(InternalError, None, id).into()
        };

        let time_diff = end_header.timestamp.inner() as f64 - start_header.timestamp.inner() as f64;
        let blocks_mined = (height - start_height) as f64;

        if time_diff <= 0.0 || blocks_mined <= 0.0 {
            return JsonResponse::new(JsonValue::Number(0.0), id).into()
        }

        // Get current difficulty
        let Ok(Some(diff)) = self.get_difficulty(height) else {
            return JsonError::new(InternalError, None, id).into()
        };

        // Average block time in seconds
        let avg_block_time = time_diff / blocks_mined;

        // Hashrate = difficulty / block_time
        // This approximates hashes per second needed to find a block at current difficulty
        let hashrate = (diff.difficulty as f64) / avg_block_time;

        JsonResponse::new(JsonValue::Number(hashrate), id).into()
    }
}
