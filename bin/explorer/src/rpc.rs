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

struct LatestBlockInfo {
    height: u64,
    size: u64,
    n_txs: u64,
    timestamp: u64,
    powtype: String,
    hash: String,
}

impl LatestBlockInfo {
    async fn new(block: &BlockInfo) -> Self {
        let powtype = match block.header.pow_data {
            PowData::DarkFi => "DarkFi".to_string(),
            PowData::Monero(_) => "Monero".to_string(),
        };

        Self {
            height: block.header.height as u64,
            size: serialize_async(block).await.len() as u64,
            n_txs: block.txs.len() as u64,
            timestamp: block.header.timestamp.inner(),
            powtype,
            hash: block.header.hash().to_string(),
        }
    }

    fn to_json(&self) -> JsonValue {
        JsonValue::Object(HashMap::from([
            ("height".to_string(), JsonValue::Number(self.height as f64)),
            ("size".to_string(), JsonValue::Number(self.size as f64)),
            ("n_txs".to_string(), JsonValue::Number(self.n_txs as f64)),
            ("timestamp".to_string(), JsonValue::Number(self.timestamp as f64)),
            ("powtype".to_string(), JsonValue::String(self.powtype.clone())),
            ("hash".to_string(), JsonValue::String(self.hash.clone())),
        ]))
    }
}

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

        let (mut blocks, n_blocks) = if n_blocks > height {
            (Vec::with_capacity(height as usize), height)
        } else {
            (Vec::with_capacity(n_blocks as usize), n_blocks)
        };

        for i in (0..=n_blocks).rev() {
            let Ok(Some(block)) = self.get_block(i).await else {
                return JsonError::new(InternalError, None, id).into()
            };

            blocks.push(LatestBlockInfo::new(&block).await.to_json());
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
}
