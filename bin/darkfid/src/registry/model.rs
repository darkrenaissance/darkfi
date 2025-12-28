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

use std::{collections::HashMap, str::FromStr};

use rand::rngs::OsRng;
use tinyjson::JsonValue;
use tracing::info;

use darkfi::{
    blockchain::{BlockInfo, Header, HeaderHash},
    rpc::jsonrpc::JsonSubscriber,
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::{
        encoding::base64,
        time::{NanoTimestamp, Timestamp},
    },
    validator::{consensus::Fork, verification::apply_producer_transaction, ValidatorPtr},
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_money_contract::{
    client::pow_reward_v1::PoWRewardCallBuilder, MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        keypair::{Address, Keypair, Network, SecretKey},
        pasta_prelude::PrimeField,
        FuncId, MerkleTree, MONEY_CONTRACT_ID,
    },
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::{deserialize_async, Encodable};

use crate::error::RpcError;

/// Auxiliary structure representing node miner rewards recipient configuration.
#[derive(Debug, Clone)]
pub struct MinerRewardsRecipientConfig {
    /// Wallet mining address to receive mining rewards
    pub recipient: Address,
    /// Optional contract spend hook to use in the mining reward
    pub spend_hook: Option<FuncId>,
    /// Optional contract user data to use in the mining reward.
    /// This is not arbitrary data.
    pub user_data: Option<pallas::Base>,
}

impl MinerRewardsRecipientConfig {
    pub fn new(
        recipient: Address,
        spend_hook: Option<FuncId>,
        user_data: Option<pallas::Base>,
    ) -> Self {
        Self { recipient, spend_hook, user_data }
    }

    pub async fn from_base64(
        network: &Network,
        encoded_address: &str,
    ) -> std::result::Result<Self, RpcError> {
        let Some(address_bytes) = base64::decode(encoded_address) else {
            return Err(RpcError::MinerInvalidWalletConfig)
        };
        let Ok((recipient, spend_hook, user_data)) =
            deserialize_async::<(String, Option<String>, Option<String>)>(&address_bytes).await
        else {
            return Err(RpcError::MinerInvalidWalletConfig)
        };
        let Ok(recipient) = Address::from_str(&recipient) else {
            return Err(RpcError::MinerInvalidRecipient)
        };
        if recipient.network() != *network {
            return Err(RpcError::MinerInvalidRecipientPrefix)
        }
        let spend_hook = match spend_hook {
            Some(s) => match FuncId::from_str(&s) {
                Ok(s) => Some(s),
                Err(_) => return Err(RpcError::MinerInvalidSpendHook),
            },
            None => None,
        };
        let user_data: Option<pallas::Base> = match user_data {
            Some(u) => {
                let Ok(bytes) = bs58::decode(&u).into_vec() else {
                    return Err(RpcError::MinerInvalidUserData)
                };
                let bytes: [u8; 32] = match bytes.try_into() {
                    Ok(b) => b,
                    Err(_) => return Err(RpcError::MinerInvalidUserData),
                };
                match pallas::Base::from_repr(bytes).into() {
                    Some(v) => Some(v),
                    None => return Err(RpcError::MinerInvalidUserData),
                }
            }
            None => None,
        };

        Ok(Self { recipient, spend_hook, user_data })
    }
}

/// Auxiliary structure representing a block template for mining.
#[derive(Debug, Clone)]
pub struct BlockTemplate {
    /// Block that is being mined
    pub block: BlockInfo,
    /// RandomX current and next keys pair
    pub randomx_keys: (HeaderHash, HeaderHash),
    /// Compacted block mining target
    pub target: Vec<u8>,
    /// Block difficulty
    pub difficulty: f64,
    /// Ephemeral signing secret for this blocktemplate
    pub secret: SecretKey,
    /// Flag indicating if this template has been submitted
    pub submitted: bool,
}

impl BlockTemplate {
    fn new(
        block: BlockInfo,
        randomx_keys: (HeaderHash, HeaderHash),
        target: Vec<u8>,
        difficulty: f64,
        secret: SecretKey,
    ) -> Self {
        Self { block, randomx_keys, target, difficulty, secret, submitted: false }
    }

    pub fn job_notification(&self) -> (String, JsonValue) {
        let block_hash = hex::encode(self.block.header.hash().inner()).to_string();
        let mut job = HashMap::from([
            (
                "blob".to_string(),
                JsonValue::from(hex::encode(self.block.header.to_block_hashing_blob()).to_string()),
            ),
            ("job_id".to_string(), JsonValue::from(block_hash.clone())),
            ("height".to_string(), JsonValue::from(self.block.header.height as f64)),
            ("target".to_string(), JsonValue::from(hex::encode(&self.target))),
            ("algo".to_string(), JsonValue::from(String::from("rx/0"))),
            (
                "seed_hash".to_string(),
                JsonValue::from(hex::encode(self.randomx_keys.0.inner()).to_string()),
            ),
        ]);
        if self.randomx_keys.0 != self.randomx_keys.1 {
            job.insert(
                "next_seed_hash".to_string(),
                JsonValue::from(hex::encode(self.randomx_keys.1.inner()).to_string()),
            );
        }
        (block_hash, JsonValue::from(job))
    }
}

/// Auxiliary structure representing a native miner client record.
#[derive(Debug, Clone)]
pub struct MinerClient {
    /// Miner wallet template key
    pub wallet: String,
    /// Miner recipient configuration
    pub config: MinerRewardsRecipientConfig,
    /// Current mining job
    pub job: String,
    /// Connection publisher to push new jobs
    pub publisher: JsonSubscriber,
}

impl MinerClient {
    pub fn new(wallet: &str, config: &MinerRewardsRecipientConfig, job: &str) -> (String, Self) {
        let mut hasher = blake3::Hasher::new();
        hasher.update(wallet.as_bytes());
        hasher.update(&NanoTimestamp::current_time().inner().to_le_bytes());
        let client_id = hex::encode(hasher.finalize().as_bytes()).to_string();
        let publisher = JsonSubscriber::new("job");
        (
            client_id,
            Self {
                wallet: String::from(wallet),
                config: config.clone(),
                job: job.to_owned(),
                publisher,
            },
        )
    }
}

/// ZK data used to generate the "coinbase" transaction in a block
pub struct PowRewardV1Zk {
    pub zkbin: ZkBinary,
    pub provingkey: ProvingKey,
}

impl PowRewardV1Zk {
    pub fn new(validator: &ValidatorPtr) -> Result<Self> {
        info!(
            target: "darkfid::registry::model::PowRewardV1Zk::new",
            "Generating PowRewardV1 ZkCircuit and ProvingKey...",
        );

        let (zkbin, _) = validator.blockchain.contracts.get_zkas(
            &validator.blockchain.sled_db,
            &MONEY_CONTRACT_ID,
            MONEY_CONTRACT_ZKAS_MINT_NS_V1,
        )?;

        let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
        let provingkey = ProvingKey::build(zkbin.k, &circuit);

        Ok(Self { zkbin, provingkey })
    }
}

/// Auxiliary function to generate next mining block template, in an
/// atomic manner.
pub async fn generate_next_block_template(
    extended_fork: &mut Fork,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    verify_fees: bool,
) -> Result<BlockTemplate> {
    // Grab forks' last block proposal(previous)
    let last_proposal = extended_fork.last_proposal()?;

    // Grab forks' next block height
    let next_block_height = last_proposal.block.header.height + 1;

    // Grab forks' next mine target and difficulty
    let (target, difficulty) = extended_fork.module.next_mine_target_and_difficulty()?;

    // The target should be compacted to 8 bytes little-endian.
    let target = target.to_bytes_le()[..8].to_vec();

    // Cast difficulty to f64. This should always work.
    let difficulty = difficulty.to_string().parse()?;

    // Grab forks' unproposed transactions
    let (mut txs, _, fees) = extended_fork.unproposed_txs(next_block_height, verify_fees).await?;

    // Create an ephemeral block signing keypair. Its secret key will
    // be stored in the PowReward transaction's encrypted note for
    // later retrieval. It is encrypted towards the recipient's public
    // key.
    let block_signing_keypair = Keypair::random(&mut OsRng);

    // Generate reward transaction
    let tx = generate_transaction(
        next_block_height,
        fees,
        &block_signing_keypair,
        recipient_config,
        zkbin,
        pk,
    )?;

    // Apply producer transaction in the forks' overlay
    let _ = apply_producer_transaction(
        &extended_fork.overlay,
        next_block_height,
        extended_fork.module.target,
        &tx,
        &mut MerkleTree::new(1),
    )
    .await?;
    txs.push(tx);

    // Grab the updated contracts states root
    let diff =
        extended_fork.overlay.lock().unwrap().overlay.lock().unwrap().diff(&extended_fork.diffs)?;
    extended_fork
        .overlay
        .lock()
        .unwrap()
        .contracts
        .update_state_monotree(&diff, &mut extended_fork.state_monotree)?;
    let Some(state_root) = extended_fork.state_monotree.get_headroot()? else {
        return Err(Error::ContractsStatesRootNotFoundError);
    };

    // Generate the new header
    let mut header =
        Header::new(last_proposal.hash, next_block_height, 0, Timestamp::current_time());
    header.state_root = state_root;

    // Generate the block
    let mut next_block = BlockInfo::new_empty(header);

    // Add transactions to the block
    next_block.append_txs(txs);

    Ok(BlockTemplate::new(
        next_block,
        extended_fork.module.darkfi_rx_keys,
        target,
        difficulty,
        block_signing_keypair.secret,
    ))
}

/// Auxiliary function to generate a Money::PoWReward transaction.
fn generate_transaction(
    block_height: u32,
    fees: u64,
    block_signing_keypair: &Keypair,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<Transaction> {
    // Build the transaction debris
    let debris = PoWRewardCallBuilder {
        signature_keypair: *block_signing_keypair,
        block_height,
        fees,
        recipient: Some(*recipient_config.recipient.public_key()),
        spend_hook: recipient_config.spend_hook,
        user_data: recipient_config.user_data,
        mint_zkbin: zkbin.clone(),
        mint_pk: pk.clone(),
    }
    .build()?;

    // Generate and sign the actual transaction
    let mut data = vec![MoneyFunction::PoWRewardV1 as u8];
    debris.params.encode(&mut data)?;
    let call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };
    let mut tx_builder =
        TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
    let mut tx = tx_builder.build()?;
    let sigs = tx.create_sigs(&[block_signing_keypair.secret])?;
    tx.signatures = vec![sigs];

    Ok(tx)
}
