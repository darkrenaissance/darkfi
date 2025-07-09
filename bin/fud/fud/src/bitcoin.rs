/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{
    collections::HashMap,
    io::{Cursor, Read},
    sync::Arc,
    time::Duration,
};

use rand::{prelude::SliceRandom, rngs::OsRng};
use sha2::{Digest, Sha256};
use smol::lock::RwLock;
use tinyjson::JsonValue;
use tracing::{error, info, warn};
use url::Url;

use darkfi::{
    rpc::{client::RpcClient, jsonrpc::JsonRequest},
    system::{timeout::timeout, ExecutorPtr},
    Error, Result,
};
use darkfi_sdk::{hex::decode_hex, GenericResult};

use crate::pow::PowSettings;

pub type BitcoinBlockHash = [u8; 32];

/// A struct that can fetch and store recent Bitcoin block hashes, using Electrum nodes.
/// This is only used to evaluate and verify fud's Equi-X PoW.
/// Bitcoin block hashes are used in the challenge, to make Equi-X solution
/// expirable and unpredictable.
/// It's meant to be swapped with DarkFi block hashes once it is stable enough.
/// TODO: It should ask for new Electrum nodes, and build a local database of them
/// instead of relying only on the list defined in the settings.
pub struct BitcoinHashCache {
    /// PoW settings which includes BTC/Electrum settings
    settings: Arc<RwLock<PowSettings>>,
    /// Current list of block hashes, the most recent block is at the end of the list
    pub block_hashes: Vec<BitcoinBlockHash>,
    /// Global multithreaded executor reference
    ex: ExecutorPtr,
}

impl BitcoinHashCache {
    pub fn new(settings: Arc<RwLock<PowSettings>>, ex: ExecutorPtr) -> Self {
        Self { settings, block_hashes: vec![], ex }
    }

    /// Fetch block hashes from Electrum nodes, and update [`BitcoinHashCache::block_hashes`].
    pub async fn update(&mut self) -> Result<Vec<BitcoinBlockHash>> {
        info!(target: "fud::BitcoinHashCache::update()", "[BTC] Updating block hashes...");

        let mut block_hashes = vec![];
        let btc_electrum_nodes = self.settings.read().await.btc_electrum_nodes.clone();

        let mut rng = OsRng;
        let mut shuffled_nodes: Vec<_> = btc_electrum_nodes.clone();
        shuffled_nodes.shuffle(&mut rng);

        for addr in shuffled_nodes {
            // Connect to the Electrum node
            let client = match self.create_rpc_client(&addr).await {
                Ok(client) => client,
                Err(e) => {
                    warn!(target: "fud::BitcoinHashCache::update()", "[BTC] Error while creating RPC client for Electrum node {addr}: {e}");
                    continue
                }
            };
            info!(target: "fud::BitcoinHashCache::update()", "[BTC] Connected to {addr}");

            // Fetch the current BTC height
            let current_height = match self.fetch_current_height(&client).await {
                Ok(height) => height,
                Err(e) => {
                    warn!(target: "fud::BitcoinHashCache::update()", "[BTC] Error while fetching current height: {e}");
                    client.stop().await;
                    continue
                }
            };
            info!(target: "fud::BitcoinHashCache::update()", "[BTC] Found current height {current_height}");

            // Fetch the latest block hashes
            match self.fetch_hashes(current_height, &client).await {
                Ok(hashes) => {
                    client.stop().await;
                    if !hashes.is_empty() {
                        block_hashes = hashes;
                        break
                    }
                    warn!(target: "fud::BitcoinHashCache::update()", "[BTC] The Electrum node replied with an empty list of block headers");
                    continue
                }
                Err(e) => {
                    warn!(target: "fud::BitcoinHashCache::update()", "[BTC] Error while fetching block hashes: {e}");
                    client.stop().await;
                    continue
                }
            };
        }

        if block_hashes.is_empty() {
            let err_str = "Could not find any block hash";
            error!(target: "fud::BitcoinHashCache::update()", "[BTC] {err_str}");
            return Err(Error::Custom(err_str.to_string()))
        }

        info!(target: "fud::BitcoinHashCache::update()", "[BTC] Found {} block hashes", block_hashes.len());

        self.block_hashes = block_hashes.clone();
        Ok(block_hashes)
    }

    async fn create_rpc_client(&self, addr: &Url) -> Result<RpcClient> {
        let btc_timeout = Duration::from_secs(self.settings.read().await.btc_timeout);
        let client = timeout(btc_timeout, RpcClient::new(addr.clone(), self.ex.clone())).await??;
        Ok(client)
    }

    /// Fetch the current BTC height using an Electrum node RPC.
    async fn fetch_current_height(&self, client: &RpcClient) -> Result<u64> {
        let btc_timeout = Duration::from_secs(self.settings.read().await.btc_timeout);
        let req = JsonRequest::new("blockchain.headers.subscribe", vec![].into());
        let rep = timeout(btc_timeout, client.request(req)).await??;

        rep.get::<HashMap<String, JsonValue>>()
            .and_then(|res| res.get("height"))
            .and_then(|h| h.get::<f64>())
            .map(|h| *h as u64)
            .ok_or_else(|| {
                Error::JsonParseError(
                    "Failed to parse `blockchain.headers.subscribe` response".into(),
                )
            })
    }

    /// Fetch `self.count` BTC block hashes from `height` using an Electrum node RPC.
    async fn fetch_hashes(&self, height: u64, client: &RpcClient) -> Result<Vec<BitcoinBlockHash>> {
        let count = self.settings.read().await.btc_hash_count;
        let btc_timeout = Duration::from_secs(self.settings.read().await.btc_timeout);
        let req = JsonRequest::new(
            "blockchain.block.headers",
            vec![
                JsonValue::Number((height as f64) - (count as f64)),
                JsonValue::Number(count as f64),
            ]
            .into(),
        );
        let rep = timeout(btc_timeout, client.request(req)).await??;

        let hex: &String = rep
            .get::<HashMap<String, JsonValue>>()
            .and_then(|res| res.get("hex"))
            .and_then(|h| h.get::<String>())
            .ok_or_else(|| {
                Error::JsonParseError("Failed to parse `blockchain.block.headers` response".into())
            })?;

        let decoded_bytes = decode_hex(hex.as_str()).collect::<GenericResult<Vec<_>>>()?;
        Self::decode_block_hashes(decoded_bytes)
    }

    /// Convert concatenated BTC block headers to a list of block hashes.
    fn decode_block_hashes(data: Vec<u8>) -> Result<Vec<BitcoinBlockHash>> {
        let mut cursor = Cursor::new(&data);
        let count = data.len() / 80;

        let mut hashes = Vec::with_capacity(count);
        for _ in 0..count {
            // Read the 80-byte header
            let mut header = [0u8; 80];
            cursor.read_exact(&mut header)?;

            // Compute double SHA-256
            let first_hash = Sha256::digest(header);
            let second_hash = Sha256::digest(first_hash);

            // Convert to big-endian hash
            let mut be_hash = [0u8; 32];
            be_hash.copy_from_slice(&second_hash);
            be_hash.reverse();

            hashes.push(be_hash);
        }

        Ok(hashes)
    }
}
