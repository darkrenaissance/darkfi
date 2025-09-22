/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
    io::{Error as IoError, Read, Result as IoResult, Write},
    sync::Arc,
};

use log::info;
use rand::rngs::OsRng;
use smol::lock::RwLock;
use structopt::StructOpt;
use url::Url;

use darkfi::{system::ExecutorPtr, Error, Result};
use darkfi_sdk::crypto::{Keypair, PublicKey, SecretKey};
use darkfi_serial::{
    async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite, Decodable, Encodable,
};

use crate::{
    bitcoin::{BitcoinBlockHash, BitcoinHashCache},
    equix::{Challenge, EquiXBuilder, EquiXPow, Solution, SolverMemory, NONCE_LEN},
};

#[derive(Clone, Debug)]
pub struct PowSettings {
    /// Equi-X effort value
    pub equix_effort: u32,
    /// Number of latest BTC block hashes that are valid for fud's PoW
    pub btc_hash_count: usize,
    /// Electrum nodes timeout in seconds
    pub btc_timeout: u64,
    /// Electrum nodes used to fetch the latest block hashes (used in fud's PoW)
    pub btc_electrum_nodes: Vec<Url>,
}

impl Default for PowSettings {
    fn default() -> Self {
        Self {
            equix_effort: 10000,
            btc_hash_count: 144,
            btc_timeout: 15,
            btc_electrum_nodes: vec![],
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize, structopt::StructOpt, structopt_toml::StructOptToml)]
#[structopt()]
#[serde(rename = "pow")]
pub struct PowSettingsOpt {
    /// Equi-X effort value
    #[structopt(long)]
    pub equix_effort: Option<u32>,

    /// Number of latest BTC block hashes that are valid for fud's PoW
    #[structopt(long)]
    pub btc_hash_count: Option<usize>,

    /// Electrum nodes timeout in seconds
    #[structopt(long)]
    pub btc_timeout: Option<u64>,

    /// Electrum nodes used to fetch the latest block hashes (used in fud's PoW)
    #[structopt(long, use_delimiter = true)]
    pub btc_electrum_nodes: Vec<Url>,
}

impl From<PowSettingsOpt> for PowSettings {
    fn from(opt: PowSettingsOpt) -> Self {
        let def = PowSettings::default();

        Self {
            equix_effort: opt.equix_effort.unwrap_or(def.equix_effort),
            btc_hash_count: opt.btc_hash_count.unwrap_or(def.btc_hash_count),
            btc_timeout: opt.btc_timeout.unwrap_or(def.btc_timeout),
            btc_electrum_nodes: opt.btc_electrum_nodes,
        }
    }
}

/// Struct handling a [`EquiXPow`] instance to generate and verify [`VerifiableNodeData`].
pub struct FudPow {
    pub settings: Arc<RwLock<PowSettings>>,
    pub bitcoin_hash_cache: BitcoinHashCache,
    equix_pow: EquiXPow,
}
impl FudPow {
    pub fn new(settings: PowSettings, ex: ExecutorPtr) -> Self {
        let pow_settings: Arc<RwLock<PowSettings>> = Arc::new(RwLock::new(settings));
        let bitcoin_hash_cache = BitcoinHashCache::new(pow_settings.clone(), ex.clone());

        Self {
            settings: pow_settings,
            bitcoin_hash_cache,
            equix_pow: EquiXPow {
                effort: 0, // will be set when we call `generate_node()`
                challenge: Challenge::new(&[], &[0u8; NONCE_LEN]),
                equix: EquiXBuilder::default(),
                mem: SolverMemory::default(),
            },
        }
    }

    /// Generate a random keypair and run the PoW to get a [`VerifiableNodeData`].
    pub async fn generate_node(&mut self) -> Result<(VerifiableNodeData, SecretKey)> {
        info!(target: "fud::FudPow::generate_node()", "Generating a new node id...");

        // Generate a random keypair
        let keypair = Keypair::random(&mut OsRng);

        // Get a recent Bitcoin block hash
        let n = 3;
        let btc_block_hash = {
            let block_hashes = &self.bitcoin_hash_cache.block_hashes;
            if block_hashes.is_empty() {
                return Err(Error::Custom(
                    "Can't generate a node id without BTC block hashes".into(),
                ));
            }

            let block_hash = if n > block_hashes.len() {
                block_hashes.last()
            } else {
                block_hashes.get(block_hashes.len() - 1 - n)
            };

            if block_hash.is_none() {
                return Err(Error::Custom("Could not find a recent BTC block hash".into()));
            }
            *block_hash.unwrap()
        };

        // Update the effort using the value from `self.settings`
        self.equix_pow.effort = self.settings.read().await.equix_effort;

        // Construct Equi-X challenge
        self.equix_pow.challenge = Challenge::new(
            &[keypair.public.to_bytes(), btc_block_hash].concat(),
            &[0u8; NONCE_LEN],
        );

        // Evaluate PoW
        info!(target: "fud::FudPow::generate_node()", "Equi-X Proof-of-Work starts...");
        let solution =
            self.equix_pow.run().map_err(|e| Error::Custom(format!("Equi-X error: {e}")))?;
        info!(target: "fud::FudPow::generate_node()", "Equi-X Proof-of-Work is done");

        // Create the VerifiableNodeData
        Ok((
            VerifiableNodeData {
                public_key: keypair.public,
                btc_block_hash,
                nonce: self.equix_pow.challenge.nonce(),
                solution,
            },
            keypair.secret,
        ))
    }

    /// Check if the Equi-X solution in a [`VerifiableNodeData`] is valid and has enough effort.
    pub async fn verify_node(&mut self, node_data: &VerifiableNodeData) -> Result<()> {
        // Update the effort using the value from `self.settings`
        self.equix_pow.effort = self.settings.read().await.equix_effort;

        // Verify if the Bitcoin block hash is known
        if !self.bitcoin_hash_cache.block_hashes.contains(&node_data.btc_block_hash) {
            return Err(Error::Custom(
                "Error verifying node data: the BTC block hash is unknown".into(),
            ))
        }

        // Verify the solution
        self.equix_pow
            .verify(&node_data.challenge(), &node_data.solution)
            .map_err(|e| Error::Custom(format!("Error verifying Equi-X solution: {e}")))
    }
}

/// The data needed to verify a fud PoW.
#[derive(Debug, Clone)]
pub struct VerifiableNodeData {
    pub public_key: PublicKey,
    pub btc_block_hash: BitcoinBlockHash,
    pub nonce: [u8; NONCE_LEN],
    pub solution: Solution,
}

impl VerifiableNodeData {
    /// The node id on the DHT.
    pub fn id(&self) -> blake3::Hash {
        blake3::hash(&[self.challenge().to_bytes(), self.solution.to_bytes().to_vec()].concat())
    }

    /// The Equi-X challenge.
    pub fn challenge(&self) -> Challenge {
        Challenge::new(&[self.public_key.to_bytes(), self.btc_block_hash].concat(), &self.nonce)
    }
}

impl Encodable for VerifiableNodeData {
    fn encode<S: Write>(&self, s: &mut S) -> IoResult<usize> {
        let mut len = 0;
        len += self.public_key.encode(s)?;
        len += self.btc_block_hash.encode(s)?;
        len += self.nonce.encode(s)?;
        len += self.solution.to_bytes().encode(s)?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncEncodable for VerifiableNodeData {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> IoResult<usize> {
        let mut len = 0;
        len += self.public_key.encode_async(s).await?;
        len += self.btc_block_hash.encode_async(s).await?;
        len += self.nonce.encode_async(s).await?;
        len += self.solution.to_bytes().encode_async(s).await?;
        Ok(len)
    }
}

impl Decodable for VerifiableNodeData {
    fn decode<D: Read>(d: &mut D) -> IoResult<Self> {
        Ok(Self {
            public_key: PublicKey::decode(d)?,
            btc_block_hash: BitcoinBlockHash::decode(d)?,
            nonce: <[u8; NONCE_LEN]>::decode(d)?,
            solution: Solution::try_from_bytes(&<[u8; Solution::NUM_BYTES]>::decode(d)?)
                .map_err(|e| IoError::other(format!("Error parsing Equi-X solution: {e}")))?,
        })
    }
}

#[async_trait]
impl AsyncDecodable for VerifiableNodeData {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> IoResult<Self> {
        Ok(Self {
            public_key: PublicKey::decode_async(d).await?,
            btc_block_hash: BitcoinBlockHash::decode_async(d).await?,
            nonce: <[u8; NONCE_LEN]>::decode_async(d).await?,
            solution: Solution::try_from_bytes(
                &<[u8; Solution::NUM_BYTES]>::decode_async(d).await?,
            )
            .map_err(|e| IoError::other(format!("Error parsing Equi-X solution: {e}")))?,
        })
    }
}
