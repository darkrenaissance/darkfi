/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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
    blockchain::{BlockInfo, Header},
    net::Settings,
    rpc::jsonrpc::JsonSubscriber,
    tx::Transaction,
    util::time::TimeKeeper,
    validator::{pid::slot_pid_output, Validator, ValidatorConfig},
    Result,
};
use darkfi_contract_test_harness::{vks, Holder, TestHarness};
use darkfi_sdk::{
    blockchain::{expected_reward, PidOutput, PreviousSlot, Slot},
    pasta::{group::ff::Field, pallas},
};
use url::Url;

use crate::{
    proto::BlockInfoMessage,
    task::sync::sync_task,
    utils::{genesis_txs_total, spawn_consensus_p2p, spawn_sync_p2p},
    Darkfid,
};

pub struct HarnessConfig {
    pub testing_node: bool,
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
    pub async fn new(config: HarnessConfig, ex: &Arc<smol::Executor<'static>>) -> Result<Self> {
        // Use test harness to generate genesis transactions
        let mut th = TestHarness::new(&["money".to_string(), "consensus".to_string()]).await?;
        let (genesis_stake_tx, _) = th.genesis_stake(&Holder::Alice, config.alice_initial)?;
        let (genesis_mint_tx, _) = th.genesis_mint(&Holder::Bob, config.bob_initial)?;

        // Generate default genesis block
        let mut genesis_block = BlockInfo::default();

        // Retrieve genesis producer transaction
        let producer_tx = genesis_block.txs.pop().unwrap();

        // Append genesis transactions and calculate their total
        genesis_block.txs.push(genesis_stake_tx);
        genesis_block.txs.push(genesis_mint_tx);
        genesis_block.txs.push(producer_tx);
        let genesis_txs_total = genesis_txs_total(&genesis_block.txs)?;
        genesis_block.slots[0].total_tokens = genesis_txs_total;

        // Generate validators configuration
        // NOTE: we are not using consensus constants here so we
        // don't get circular dependencies.
        let time_keeper = TimeKeeper::new(genesis_block.header.timestamp, 10, 90, 0);
        let validator_config = ValidatorConfig::new(
            time_keeper,
            genesis_block,
            genesis_txs_total,
            vec![],
            config.testing_node,
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

    pub async fn validate_chains(&self, total_blocks: usize, total_slots: usize) -> Result<()> {
        let genesis_txs_total = self.config.alice_initial + self.config.bob_initial;
        let alice = &self.alice.validator.read().await;
        let bob = &self.bob.validator.read().await;

        alice.validate_blockchain(genesis_txs_total, vec![]).await?;
        bob.validate_blockchain(genesis_txs_total, vec![]).await?;

        let alice_blockchain_len = alice.blockchain.len();
        assert_eq!(alice_blockchain_len, bob.blockchain.len());
        assert_eq!(alice_blockchain_len, total_blocks);

        let alice_slots_len = alice.blockchain.slots.len();
        assert_eq!(alice_slots_len, bob.blockchain.slots.len());
        assert_eq!(alice_slots_len, total_slots);

        Ok(())
    }

    pub async fn add_blocks(&self, blocks: &[BlockInfo]) -> Result<()> {
        // We simply broadcast the block using Alice's sync P2P
        for block in blocks {
            self.alice.sync_p2p.broadcast(&BlockInfoMessage::from(block)).await;
        }

        // and then add it to her chain
        self.alice.validator.write().await.add_blocks(blocks).await?;

        Ok(())
    }

    pub async fn generate_next_block(
        &self,
        previous: &BlockInfo,
        slots_count: usize,
    ) -> Result<BlockInfo> {
        let previous_hash = previous.blockhash();

        // Generate empty slots
        let mut slots = Vec::with_capacity(slots_count);
        let mut previous_slot = previous.slots.last().unwrap().clone();
        for i in 0..slots_count {
            let id = previous_slot.id + 1;
            // First slot in the sequence has (at least) 1 previous slot producer
            let producers = if i == 0 { 1 } else { 0 };
            let previous = PreviousSlot::new(
                producers,
                vec![previous_hash],
                vec![previous.header.previous],
                previous_slot.pid.error,
            );
            let (f, error, sigma1, sigma2) = slot_pid_output(&previous_slot, producers);
            let pid = PidOutput::new(f, error, sigma1, sigma2);
            let total_tokens = previous_slot.total_tokens + previous_slot.reward;
            // Only last slot in the sequence has a reward
            let reward = if i == slots_count - 1 { expected_reward(id) } else { 0 };
            let slot = Slot::new(id, previous, pid, pallas::Base::ZERO, total_tokens, reward);
            slots.push(slot.clone());
            previous_slot = slot;
        }

        // We increment timestamp so we don't have to use sleep
        let mut timestamp = previous.header.timestamp;
        timestamp.add(1);

        // Generate header
        let header = Header::new(
            previous_hash,
            previous.header.epoch,
            slots.last().unwrap().id,
            timestamp,
            previous.header.root,
        );

        // Generate block
        let block = BlockInfo::new(
            header,
            vec![Transaction::default()],
            previous.signature,
            previous.eta,
            slots,
        );

        Ok(block)
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
    let node = Darkfid::new(sync_p2p.clone(), consensus_p2p.clone(), validator, subscribers).await;

    sync_p2p.clone().start().await?;

    if consensus_settings.is_some() {
        let consensus_p2p = consensus_p2p.unwrap();
        consensus_p2p.clone().start().await?;
    }

    if !skip_sync {
        sync_task(&node).await?;
    } else {
        node.validator.write().await.synced = true;
    }

    node.validator.write().await.purge_pending_txs().await?;

    Ok(node)
}
