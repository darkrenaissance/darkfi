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

use std::collections::HashMap;

use num_bigint::BigUint;
use rand::rngs::OsRng;
use tracing::info;

use darkfi::{
    blockchain::{BlockInfo, Header},
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::time::Timestamp,
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
        keypair::{Address, Keypair, SecretKey},
        FuncId, MerkleTree, MONEY_CONTRACT_ID,
    },
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;

/// Auxiliary structure representing node miner rewards recipient configuration.
pub struct MinerRewardsRecipientConfig {
    /// Wallet mining address to receive mining rewards
    pub recipient: Address,
    /// Optional contract spend hook to use in the mining reward
    pub spend_hook: Option<FuncId>,
    /// Optional contract user data to use in the mining reward.
    /// This is not arbitrary data.
    pub user_data: Option<pallas::Base>,
}

/// Auxiliary structure representing a block template for native
/// mining.
#[derive(Debug, Clone)]
pub struct BlockTemplate {
    /// Block that is being mined
    pub block: BlockInfo,
    /// RandomX init key
    pub randomx_key: [u8; 32],
    /// Block mining target
    pub target: BigUint,
    /// Ephemeral signing secret for this blocktemplate
    pub secret: SecretKey,
}

/// Auxiliary structure representing a block template for merge mining.
#[derive(Debug, Clone)]
pub struct MmBlockTemplate {
    /// Block that is being mined
    pub block: BlockInfo,
    /// Block difficulty
    pub difficulty: f64,
    /// Ephemeral signing secret for this blocktemplate
    pub secret: SecretKey,
}

/// Storage for active mining jobs. These are stored per connection ID.
/// A new map will be made for each stratum login.
#[derive(Debug, Default)]
pub struct MiningJobs(HashMap<[u8; 32], BlockTemplate>);

impl MiningJobs {
    pub fn insert(&mut self, job_id: [u8; 32], blocktemplate: BlockTemplate) {
        self.0.insert(job_id, blocktemplate);
    }

    pub fn get(&self, job_id: &[u8; 32]) -> Option<&BlockTemplate> {
        self.0.get(job_id)
    }

    pub fn get_mut(&mut self, job_id: &[u8; 32]) -> Option<&mut BlockTemplate> {
        self.0.get_mut(job_id)
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

/// Auxiliary function to generate next block in an atomic manner.
pub async fn generate_next_block(
    extended_fork: &mut Fork,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    block_target: u32,
    verify_fees: bool,
) -> Result<(BigUint, BlockInfo, SecretKey)> {
    // Grab forks' last block proposal(previous)
    let last_proposal = extended_fork.last_proposal()?;

    // Grab forks' next block height
    let next_block_height = last_proposal.block.header.height + 1;

    // Grab forks' unproposed transactions
    let (mut txs, _, fees, overlay) = extended_fork
        .unproposed_txs(&extended_fork.blockchain, next_block_height, block_target, verify_fees)
        .await?;

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

    // Apply producer transaction in the overlay
    let _ = apply_producer_transaction(
        &overlay,
        next_block_height,
        block_target,
        &tx,
        &mut MerkleTree::new(1),
    )
    .await?;
    txs.push(tx);

    // Grab the updated contracts states root
    let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&extended_fork.diffs)?;
    overlay
        .lock()
        .unwrap()
        .contracts
        .update_state_monotree(&diff, &mut extended_fork.state_monotree)?;
    let Some(state_root) = extended_fork.state_monotree.get_headroot()? else {
        return Err(Error::ContractsStatesRootNotFoundError);
    };

    // Drop new trees opened by the unproposed transactions overlay
    overlay.lock().unwrap().overlay.lock().unwrap().purge_new_trees()?;

    // Generate the new header
    let mut header =
        Header::new(last_proposal.hash, next_block_height, Timestamp::current_time(), 0);
    header.state_root = state_root;

    // Generate the block
    let mut next_block = BlockInfo::new_empty(header);

    // Add transactions to the block
    next_block.append_txs(txs);

    // Grab the next mine target
    let target = extended_fork.module.next_mine_target()?;

    Ok((target, next_block, block_signing_keypair.secret))
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
