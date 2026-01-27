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
    util::{encoding::base64, parse::encode_base10},
};
use darkfi_money_contract::MoneyFunction;
use darkfi_sdk::crypto::contract_id::MONEY_CONTRACT_ID;
use darkfi_serial::{deserialize_async, serialize_async};
use monero::{consensus::encode::Encodable, VarInt};
use tiny_keccak::{Hasher, Keccak};
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
        let mut fee = 0;
        let mut calls = Vec::with_capacity(tx.calls.len());
        for call in &tx.calls {
            let func = call.data.data[0];

            if call.data.contract_id == *MONEY_CONTRACT_ID && func == MoneyFunction::FeeV1 as u8 {
                fee = deserialize_async(&call.data.data[1..9]).await.unwrap();
            }

            calls.push(ContractCallInfo::new(
                call.data.contract_id.to_string(),
                format!("0x{:02x}", func),
                call.data.data.len() as u64,
            ));
        }

        Self {
            hash: tx.hash().to_string(),
            calls,
            fee,
            size: serialize_async(tx).await.len() as u64,
        }
    }

    fn to_json(&self) -> JsonValue {
        let calls = self.calls.iter().map(|c| c.to_json()).collect();

        JsonValue::Object(HashMap::from([
            ("hash".to_string(), JsonValue::String(self.hash.clone())),
            ("calls".to_string(), JsonValue::Array(calls)),
            ("fee".to_string(), JsonValue::String(encode_base10(self.fee, 8))),
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
        let mut fee = 0;
        let mut calls = Vec::with_capacity(tx.calls.len());
        for call in &tx.calls {
            let func = call.data.data[0];

            if call.data.contract_id == *MONEY_CONTRACT_ID && func == MoneyFunction::FeeV1 as u8 {
                fee = deserialize_async(&call.data.data[1..9]).await.unwrap();
            }

            calls.push(ContractCallInfo::new(
                call.data.contract_id.to_string(),
                format!("0x{:02x}", func),
                call.data.data.len() as u64,
            ));
        }

        let raw_bytes = serialize_async(tx).await;
        let confirmations =
            if current_height >= block_height { current_height - block_height + 1 } else { 0 };

        Self {
            hash: tx.hash().to_string(),
            from_block: block_height,
            confirmations,
            fee,
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
            ("fee".to_string(), JsonValue::String(encode_base10(self.fee, 8))),
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
    monero_hash: Option<String>,
    txs: Vec<TransactionInfo>,
    coinbase: CoinbaseInfo,
}

impl ExplBlockInfo {
    async fn new(block: &BlockInfo, diff: &DifficultyIndex) -> Self {
        let mut monero_hash = None;
        let powtype = match &block.header.pow_data {
            PowData::DarkFi => "DarkFi".to_string(),
            PowData::Monero(powdata) => {
                // Calculate the Monero block header hash
                let mut blockhashing_blob = powdata.to_block_hashing_blob();
                // Monero prefixes a VarInt of the blob len before getting the
                // block hash but doesn't do this when getting the PoW hash :)
                let mut header = vec![];
                VarInt(blockhashing_blob.len() as u64).consensus_encode(&mut header).unwrap();
                header.append(&mut blockhashing_blob);

                let mut keccak = Keccak::v256();
                keccak.update(&header);

                let mut hash = [0u8; 32];
                keccak.finalize(&mut hash);

                monero_hash = Some(hex::encode(hash));

                "Monero".to_string()
            }
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
            monero_hash,
            txs,
            coinbase,
        }
    }

    fn to_json(&self) -> JsonValue {
        let monero_hash = if let Some(hash) = &self.monero_hash {
            JsonValue::String(hash.to_string())
        } else {
            JsonValue::Null
        };

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
            ("monero_hash".to_string(), monero_hash),
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

    /// Get contract information by ID.
    /// Params: `[contract_id: String]`
    /// Returns: `{ contract_id, locked, wasm_size, deploy_block, deploy_tx }`
    pub async fn rpc_get_contract(&self, id: u16, params: JsonValue) -> JsonResult {
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let contract_id_str = params[0].get::<String>().unwrap();

        let Ok(Some(contract)) = self.get_contract(contract_id_str).await else {
            return JsonError::new(InternalError, Some("Contract not found".to_string()), id).into()
        };

        JsonResponse::new(
            JsonValue::Object(HashMap::from([
                ("contract_id".to_string(), JsonValue::String(contract.contract_id.to_string())),
                ("locked".to_string(), JsonValue::Boolean(contract.locked)),
                ("wasm_size".to_string(), JsonValue::Number(contract.wasm_size as f64)),
                ("deploy_block".to_string(), JsonValue::Number(contract.deploy_block as f64)),
                ("deploy_tx".to_string(), JsonValue::String(hex::encode(contract.deploy_tx_hash))),
            ])),
            id,
        )
        .into()
    }

    /// List all contracts.
    /// Params: `[locked_filter: bool | null] (optional)`
    /// Returns: Array of contract objects
    pub async fn rpc_list_contracts(&self, id: u16, params: JsonValue) -> JsonResult {
        let locked_filter = if let Some(params) = params.get::<Vec<JsonValue>>() {
            if !params.is_empty() {
                params[0].get::<bool>().copied()
            } else {
                None
            }
        } else {
            None
        };

        let Ok(contracts) = self.list_contracts(locked_filter).await else {
            return JsonError::new(InternalError, None, id).into()
        };

        let contracts_json: Vec<JsonValue> = contracts
            .iter()
            .map(|c| {
                JsonValue::Object(HashMap::from([
                    ("contract_id".to_string(), JsonValue::String(c.contract_id.to_string())),
                    ("locked".to_string(), JsonValue::Boolean(c.locked)),
                    ("wasm_size".to_string(), JsonValue::Number(c.wasm_size as f64)),
                    ("deploy_block".to_string(), JsonValue::Number(c.deploy_block as f64)),
                    ("deploy_tx".to_string(), JsonValue::String(hex::encode(c.deploy_tx_hash))),
                ]))
            })
            .collect();

        JsonResponse::new(JsonValue::Array(contracts_json), id).into()
    }

    /// Get contract count.
    /// Returns: Number of contracts
    pub async fn rpc_contract_count(&self, id: u16, _params: JsonValue) -> JsonResult {
        let Ok(count) = self.get_contract_count() else {
            return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(JsonValue::Number(count as f64), id).into()
    }

    /// Get blockchain statistics from stored data.
    /// Returns daily stats, monthly growth, and tx per block stats.
    pub async fn rpc_get_stats(&self, id: u16, _params: JsonValue) -> JsonResult {
        // Get daily stats from sled
        let daily_stats = match self.get_all_daily_stats().await {
            Ok(stats) => stats,
            Err(_) => return JsonError::new(InternalError, None, id).into(),
        };

        // Get monthly stats from sled
        let monthly_stats = match self.get_all_monthly_stats().await {
            Ok(stats) => stats,
            Err(_) => return JsonError::new(InternalError, None, id).into(),
        };

        // Convert daily stats to JSON (for graph)
        let daily_json: Vec<JsonValue> = daily_stats
            .iter()
            .map(|(day, stats)| {
                let avg_tx = if stats.block_count > 0 {
                    stats.user_tx_count as f64 / stats.block_count as f64
                } else {
                    0.0
                };
                JsonValue::Object(HashMap::from([
                    ("day".to_string(), JsonValue::Number(*day as f64)),
                    ("avg_tx".to_string(), JsonValue::Number(avg_tx)),
                    ("block_count".to_string(), JsonValue::Number(stats.block_count as f64)),
                    ("user_tx_count".to_string(), JsonValue::Number(stats.user_tx_count as f64)),
                    ("total_size".to_string(), JsonValue::Number(stats.total_size as f64)),
                ]))
            })
            .collect();

        // Convert monthly stats to JSON with cumulative
        let mut cumulative: u64 = 0;
        let monthly_json: Vec<JsonValue> = monthly_stats
            .iter()
            .map(|(year, month, stats)| {
                cumulative += stats.total_size;
                JsonValue::Object(HashMap::from([
                    ("year".to_string(), JsonValue::Number(*year as f64)),
                    ("month".to_string(), JsonValue::Number(*month as f64)),
                    (
                        "size_mb".to_string(),
                        JsonValue::Number(stats.total_size as f64 / 1_048_576.0),
                    ),
                    (
                        "cumulative_mb".to_string(),
                        JsonValue::Number(cumulative as f64 / 1_048_576.0),
                    ),
                    ("block_count".to_string(), JsonValue::Number(stats.block_count as f64)),
                ]))
            })
            .collect();

        // Calculate tx per block stats for time periods
        // Get current day
        let current_day = if let Some((day, _)) = daily_stats.last() { *day } else { 0 };

        fn calc_period_stats(
            daily_stats: &[(u64, crate::db::DailyStats)],
            from_day: u64,
            to_day: u64,
        ) -> (f64, f64, u64, u64) {
            let mut total_blocks: u64 = 0;
            let mut total_user_tx: u64 = 0;
            let mut empty_blocks: u64 = 0;

            for (day, stats) in daily_stats {
                if *day >= from_day && *day <= to_day {
                    total_blocks += stats.block_count;
                    total_user_tx += stats.user_tx_count;
                    // A block is "empty" if it has 0 user transactions
                    // We approximate empty blocks as: blocks where avg user_tx < 1
                    // But we don't have per-block data, so we estimate
                    if stats.block_count > 0 && stats.user_tx_count == 0 {
                        empty_blocks += stats.block_count;
                    }
                }
            }

            let avg_tx =
                if total_blocks > 0 { total_user_tx as f64 / total_blocks as f64 } else { 0.0 };
            let empty_pct = if total_blocks > 0 {
                empty_blocks as f64 / total_blocks as f64 * 100.0
            } else {
                0.0
            };

            (avg_tx, empty_pct, total_user_tx, total_blocks)
        }

        let (avg_day, empty_day, total_day, blocks_day) =
            calc_period_stats(&daily_stats, current_day, current_day);
        let (avg_week, empty_week, total_week, blocks_week) =
            calc_period_stats(&daily_stats, current_day.saturating_sub(6), current_day);
        let (avg_month, empty_month, total_month, blocks_month) =
            calc_period_stats(&daily_stats, current_day.saturating_sub(29), current_day);
        let (avg_year, empty_year, total_year, blocks_year) =
            calc_period_stats(&daily_stats, current_day.saturating_sub(364), current_day);

        fn stats_to_json(avg: f64, empty_pct: f64, total: u64, block_count: u64) -> JsonValue {
            JsonValue::Object(HashMap::from([
                ("avg_tx".to_string(), JsonValue::Number(avg)),
                ("empty_pct".to_string(), JsonValue::Number(empty_pct)),
                ("total_tx".to_string(), JsonValue::Number(total as f64)),
                ("block_count".to_string(), JsonValue::Number(block_count as f64)),
            ]))
        }

        let tx_per_block = JsonValue::Object(HashMap::from([
            ("last_day".to_string(), stats_to_json(avg_day, empty_day, total_day, blocks_day)),
            ("last_week".to_string(), stats_to_json(avg_week, empty_week, total_week, blocks_week)),
            (
                "last_month".to_string(),
                stats_to_json(avg_month, empty_month, total_month, blocks_month),
            ),
            ("last_year".to_string(), stats_to_json(avg_year, empty_year, total_year, blocks_year)),
        ]));

        JsonResponse::new(
            JsonValue::Object(HashMap::from([
                ("daily_stats".to_string(), JsonValue::Array(daily_json)),
                ("monthly_growth".to_string(), JsonValue::Array(monthly_json)),
                ("tx_per_block".to_string(), tx_per_block),
            ])),
            id,
        )
        .into()
    }
}
