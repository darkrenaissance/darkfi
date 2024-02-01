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

use std::{collections::HashMap, sync::Arc};

use darkfi::{
    blockchain::BlockInfo,
    net::Settings,
    rpc::jsonrpc::JsonSubscriber,
    util::time::TimeKeeper,
    validator::{utils::genesis_txs_total, Validator, ValidatorConfig},
    Result,
};
use darkfi_contract_test_harness::{vks, Holder, TestHarness};
use num_bigint::BigUint;
use url::Url;

use crate::{
    proto::BlockInfoMessage,
    task::sync::sync_task,
    utils::{spawn_consensus_p2p, spawn_sync_p2p},
    Darkfid,
};

pub struct HarnessConfig {
    pub pow_target: usize,
    pub pow_fixed_difficulty: Option<BigUint>,
    pub pos_testing_mode: bool,
    pub alice_initial: u64,
    pub bob_initial: u64,
}

pub struct Harness {
    pub config: HarnessConfig,
    pub vks: Vec<(Vec<u8>, String, Vec<u8>)>,
    pub validator_config: ValidatorConfig,
    pub alice: Darkfid,
    pub bob: Darkfid,
}

impl Harness {
    pub async fn new(
        config: HarnessConfig,
        verify_fees: bool,
        ex: &Arc<smol::Executor<'static>>,
    ) -> Result<Self> {
        // Use test harness to generate genesis transactions
        let mut th = TestHarness::new(&["money".to_string()], verify_fees).await?;
        let (genesis_mint_tx, _) = th.genesis_mint(&Holder::Bob, config.bob_initial)?;

        // Generate default genesis block
        let mut genesis_block = BlockInfo::default();

        // Append genesis transactions and calculate their total
        genesis_block.txs.push(genesis_mint_tx);
        let genesis_txs_total = genesis_txs_total(&genesis_block.txs).await?;

        // Generate validators configuration
        // NOTE: we are not using consensus constants here so we
        // don't get circular dependencies.
        let time_keeper = TimeKeeper::new(genesis_block.header.timestamp, 10, 90, 0);
        let validator_config = ValidatorConfig::new(
            time_keeper,
            3,
            config.pow_target,
            config.pow_fixed_difficulty.clone(),
            genesis_block,
            genesis_txs_total,
            vec![],
            config.pos_testing_mode,
            verify_fees,
        );

        // Generate validators using pregenerated vks
        let (_, vks) = vks::read_or_gen_vks_and_pks()?;
        let mut sync_settings =
            Settings { localnet: true, inbound_connections: 3, ..Default::default() };
        let mut consensus_settings =
            Settings { localnet: true, inbound_connections: 3, ..Default::default() };

        // Alice
        let alice_url = Url::parse("tcp+tls://127.0.0.1:18340")?;
        sync_settings.inbound_addrs = vec![alice_url.clone()];
        let alice_consensus_url = Url::parse("tcp+tls://127.0.0.1:18350")?;
        consensus_settings.inbound_addrs = vec![alice_consensus_url.clone()];
        let alice = generate_node(
            &vks,
            &validator_config,
            &sync_settings,
            Some(&consensus_settings),
            ex,
            true,
        )
        .await?;

        // Bob
        let bob_url = Url::parse("tcp+tls://127.0.0.1:18341")?;
        sync_settings.inbound_addrs = vec![bob_url];
        sync_settings.peers = vec![alice_url];
        let bob_consensus_url = Url::parse("tcp+tls://127.0.0.1:18351")?;
        consensus_settings.inbound_addrs = vec![bob_consensus_url];
        consensus_settings.peers = vec![alice_consensus_url];
        let bob = generate_node(
            &vks,
            &validator_config,
            &sync_settings,
            Some(&consensus_settings),
            ex,
            false,
        )
        .await?;

        Ok(Self { config, vks, validator_config, alice, bob })
    }

    pub async fn validate_chains(&self, total_blocks: usize) -> Result<()> {
        let alice = &self.alice.validator;
        let bob = &self.bob.validator;

        alice
            .validate_blockchain(
                vec![],
                self.config.pow_target,
                self.config.pow_fixed_difficulty.clone(),
            )
            .await?;
        bob.validate_blockchain(
            vec![],
            self.config.pow_target,
            self.config.pow_fixed_difficulty.clone(),
        )
        .await?;

        let alice_blockchain_len = alice.blockchain.len();
        assert_eq!(alice_blockchain_len, bob.blockchain.len());
        assert_eq!(alice_blockchain_len, total_blocks);

        Ok(())
    }

    pub async fn _add_blocks(&self, blocks: &[BlockInfo]) -> Result<()> {
        // We simply broadcast the block using Alice's sync P2P
        for block in blocks {
            self.alice.sync_p2p.broadcast(&BlockInfoMessage::from(block)).await;
        }

        // and then add it to her chain
        self.alice.validator.add_blocks(blocks).await?;

        Ok(())
    }
}

// Note: This function should mirror darkfid::main
pub async fn generate_node(
    vks: &Vec<(Vec<u8>, String, Vec<u8>)>,
    config: &ValidatorConfig,
    sync_settings: &Settings,
    consensus_settings: Option<&Settings>,
    ex: &Arc<smol::Executor<'static>>,
    skip_sync: bool,
) -> Result<Darkfid> {
    let sled_db = sled::Config::new().temporary(true).open()?;
    vks::inject(&sled_db, vks)?;

    let validator = Validator::new(&sled_db, config.clone()).await?;

    let mut subscribers = HashMap::new();
    subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
    subscribers.insert("txs", JsonSubscriber::new("blockchain.subscribe_txs"));
    if consensus_settings.is_some() {
        subscribers.insert("proposals", JsonSubscriber::new("blockchain.subscribe_proposals"));
    }

    let sync_p2p = spawn_sync_p2p(sync_settings, &validator, &subscribers, ex.clone()).await;
    let consensus_p2p = if let Some(settings) = consensus_settings {
        Some(spawn_consensus_p2p(settings, &validator, &subscribers, ex.clone()).await)
    } else {
        None
    };
    let node =
        Darkfid::new(sync_p2p.clone(), consensus_p2p.clone(), validator, subscribers, None).await;

    sync_p2p.clone().start().await?;

    if consensus_settings.is_some() {
        let consensus_p2p = consensus_p2p.unwrap();
        consensus_p2p.clone().start().await?;
    }

    if !skip_sync {
        sync_task(&node).await?;
    } else {
        *node.validator.synced.write().await = true;
    }

    node.validator.purge_pending_txs().await?;

    Ok(node)
}
