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

use std::{collections::HashMap, sync::Arc};

use darkfi::{
    blockchain::{BlockInfo, Blockchain, BlockchainOverlay, Header, HeaderHash},
    net::Settings,
    rpc::jsonrpc::JsonSubscriber,
    system::sleep,
    tx::{ContractCallLeaf, TransactionBuilder},
    validator::{
        consensus::{Fork, Proposal},
        utils::deploy_native_contracts,
        verification::{apply_producer_transaction, verify_block},
        Validator, ValidatorConfig,
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    Result,
};
use darkfi_contract_test_harness::vks;
use darkfi_money_contract::{
    client::pow_reward_v1::PoWRewardCallBuilder, MoneyFunction, MONEY_CONTRACT_ZKAS_MINT_NS_V1,
};
use darkfi_sdk::{
    crypto::{
        keypair::{Keypair, Network},
        MerkleTree, MONEY_CONTRACT_ID,
    },
    ContractCall,
};
use darkfi_serial::Encodable;
use num_bigint::BigUint;
use sled_overlay::sled;
use url::Url;

use crate::{
    proto::{DarkfidP2pHandler, ProposalMessage},
    registry::DarkfiMinersRegistry,
    task::sync::sync_task,
    DarkfiNode, DarkfiNodePtr,
};

pub struct HarnessConfig {
    pub pow_target: u32,
    pub pow_fixed_difficulty: Option<BigUint>,
    pub confirmation_threshold: usize,
    pub alice_url: String,
    pub bob_url: String,
}

pub struct Harness {
    pub config: HarnessConfig,
    pub vks: Vec<(Vec<u8>, String, Vec<u8>)>,
    pub validator_config: ValidatorConfig,
    pub alice: DarkfiNodePtr,
    pub bob: DarkfiNodePtr,
}

impl Harness {
    pub async fn new(
        config: HarnessConfig,
        verify_fees: bool,
        ex: &Arc<smol::Executor<'static>>,
    ) -> Result<Self> {
        // Generate default genesis block
        let mut genesis_block = BlockInfo::default();

        // Retrieve genesis producer transaction
        let producer_tx = genesis_block.txs.pop().unwrap();

        // Append it again so its added to the merkle tree
        genesis_block.append_txs(vec![producer_tx]);

        // Compute genesis contracts states monotree root
        let sled_db = sled::Config::new().temporary(true).open()?;
        let overlay = BlockchainOverlay::new(&Blockchain::new(&sled_db)?)?;
        let (_, vks) = vks::get_cached_pks_and_vks()?;
        vks::inject(&overlay, &vks)?;
        deploy_native_contracts(&overlay, config.pow_target).await?;
        let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&[])?;
        genesis_block.header.state_root =
            overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;

        // Generate validators configuration
        // NOTE: we are not using consensus constants here so we
        // don't get circular dependencies.
        let validator_config = ValidatorConfig {
            confirmation_threshold: config.confirmation_threshold,
            pow_target: config.pow_target,
            pow_fixed_difficulty: config.pow_fixed_difficulty.clone(),
            genesis_block,
            verify_fees,
        };

        // Generate validators
        let mut settings =
            Settings { localnet: true, inbound_connections: 3, ..Default::default() };

        // Alice
        let alice_url = Url::parse(&config.alice_url)?;
        settings.inbound_addrs = vec![alice_url.clone()];
        let alice = generate_node(&vks, &validator_config, &settings, ex, true, None).await?;

        // Bob
        let bob_url = Url::parse(&config.bob_url)?;
        settings.inbound_addrs = vec![bob_url];
        settings.peers = vec![alice_url];
        let bob = generate_node(&vks, &validator_config, &settings, ex, false, None).await?;

        Ok(Self { config, vks, validator_config, alice, bob })
    }

    pub async fn validate_chains(&self, total_blocks: usize) -> Result<()> {
        let alice = &self.alice.validator.read().await;
        let bob = &self.bob.validator.read().await;

        alice
            .validate_blockchain(self.config.pow_target, self.config.pow_fixed_difficulty.clone())
            .await?;

        bob.validate_blockchain(self.config.pow_target, self.config.pow_fixed_difficulty.clone())
            .await?;

        let alice_blockchain_len = alice.blockchain.len();
        assert_eq!(alice_blockchain_len, bob.blockchain.len());
        assert_eq!(alice_blockchain_len, total_blocks);
        assert!(alice.blockchain.headers.is_empty_sync());
        assert!(bob.blockchain.headers.is_empty_sync());

        Ok(())
    }

    pub async fn validate_fork_chains(&self, total_forks: usize, fork_sizes: Vec<usize>) {
        let alice = &self.alice.validator.read().await.consensus.forks;
        let bob = &self.bob.validator.read().await.consensus.forks;

        let alice_forks_len = alice.len();
        assert_eq!(alice_forks_len, bob.len());
        assert_eq!(alice_forks_len, total_forks);

        for (index, fork) in alice.iter().enumerate() {
            assert_eq!(fork.proposals.len(), fork_sizes[index]);
            assert_eq!(fork.diffs.len(), fork_sizes[index]);
            assert!(fork.healthcheck().is_ok());
        }

        for (index, fork) in bob.iter().enumerate() {
            assert_eq!(fork.proposals.len(), fork_sizes[index]);
            assert_eq!(fork.diffs.len(), fork_sizes[index]);
            assert!(fork.healthcheck().is_ok());
        }
    }

    pub async fn add_blocks(&self, blocks: &[BlockInfo]) -> Result<()> {
        // We append the block as a proposal to Alice,
        // and then we broadcast it to rest nodes
        for block in blocks {
            let proposal = Proposal::new(block.clone());
            self.alice.validator.write().await.append_proposal(&proposal).await?;
            let message = ProposalMessage(proposal);
            self.alice.p2p_handler.p2p.broadcast(&message).await;
        }

        // Sleep a bit so blocks can be propagated and then
        // trigger confirmation check to Alice and Bob
        sleep(10).await;
        self.alice.validator.write().await.confirmation().await?;
        self.bob.validator.write().await.confirmation().await?;

        Ok(())
    }

    pub async fn generate_next_block(&self, fork: &mut Fork) -> Result<BlockInfo> {
        // Grab fork last block
        let previous = fork.overlay.lock().unwrap().last_block()?;

        // Next block info
        let block_height = previous.header.height + 1;
        let last_nonce = previous.header.nonce;

        // Generate a producer transaction
        let keypair = Keypair::default();
        let (zkbin, _) = fork
            .overlay
            .lock()
            .unwrap()
            .contracts
            .get_zkas(&MONEY_CONTRACT_ID, MONEY_CONTRACT_ZKAS_MINT_NS_V1)?;
        let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
        let pk = ProvingKey::build(zkbin.k, &circuit);

        // Build the transaction debris
        let debris = PoWRewardCallBuilder {
            signature_keypair: keypair,
            block_height,
            fees: 0,
            recipient: None,
            spend_hook: None,
            user_data: None,
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
        let sigs = tx.create_sigs(&[keypair.secret])?;
        tx.signatures = vec![sigs];

        // We increment timestamp so we don't have to use sleep
        let timestamp = previous.header.timestamp.checked_add(1.into())?;

        // Generate header
        let header = Header::new(previous.hash(), block_height, last_nonce, timestamp);

        // Generate the block
        let mut block = BlockInfo::new_empty(header);

        // Add producer transaction to the block
        block.append_txs(vec![tx]);

        // Compute block contracts states monotree root
        let overlay = fork.overlay.lock().unwrap().full_clone()?;
        let _ = apply_producer_transaction(
            &overlay,
            block.header.height,
            fork.module.target,
            block.txs.last().unwrap(),
            &mut MerkleTree::new(1),
        )
        .await?;
        let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&fork.diffs)?;
        block.header.state_root = overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;

        // Attach signature
        block.sign(&keypair.secret);

        // Append new block to fork
        verify_block(
            &fork.overlay,
            &fork.diffs,
            &mut fork.module,
            &block,
            &previous,
            self.alice.validator.read().await.verify_fees,
        )
        .await?;
        fork.append_proposal(&Proposal::new(block.clone())).await?;

        Ok(block)
    }
}

// Note: This function should mirror `darkfid::Darkfid::init`
pub async fn generate_node(
    vks: &Vec<(Vec<u8>, String, Vec<u8>)>,
    config: &ValidatorConfig,
    settings: &Settings,
    ex: &Arc<smol::Executor<'static>>,
    skip_sync: bool,
    checkpoint: Option<(u32, HeaderHash)>,
) -> Result<DarkfiNodePtr> {
    let sled_db = sled::Config::new().temporary(true).open()?;
    let overlay = BlockchainOverlay::new(&Blockchain::new(&sled_db)?)?;
    vks::inject(&overlay, vks)?;
    deploy_native_contracts(&overlay, config.pow_target).await?;
    let diff = overlay.lock().unwrap().overlay.lock().unwrap().diff(&[])?;
    overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;
    overlay.lock().unwrap().overlay.lock().unwrap().apply()?;
    let validator = Validator::new(&sled_db, config).await?;

    let mut subscribers = HashMap::new();
    subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
    subscribers.insert("txs", JsonSubscriber::new("blockchain.subscribe_txs"));
    subscribers.insert("proposals", JsonSubscriber::new("blockchain.subscribe_proposals"));
    subscribers.insert("dnet", JsonSubscriber::new("dnet.subscribe_events"));

    let p2p_handler = DarkfidP2pHandler::init(settings, ex).await?;
    let registry = DarkfiMinersRegistry::init(Network::Mainnet, &validator).await?;
    let node =
        DarkfiNode::new(validator.clone(), p2p_handler.clone(), registry, 50, subscribers.clone())
            .await?;

    p2p_handler.start(ex, &node).await?;

    node.validator.write().await.consensus.generate_empty_fork().await?;

    if !skip_sync {
        sync_task(&node, checkpoint).await?;
    } else {
        node.validator.write().await.synced = true;
    }

    node.validator.write().await.purge_pending_txs().await?;

    Ok(node)
}
