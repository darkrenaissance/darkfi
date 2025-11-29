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

use std::sync::Arc;

use darkfi::{
    net::Settings,
    rpc::settings::RpcSettings,
    util::logger::{setup_test_logger, Level},
    validator::{consensus::Fork, utils::best_fork_index, verification::verify_block},
    Result,
};
use darkfi_contract_test_harness::init_logger;
use darkfi_sdk::num_traits::One;
use num_bigint::BigUint;
use smol::Executor;
use tracing::warn;
use url::Url;

mod harness;
use harness::{generate_node, Harness, HarnessConfig};

mod forks;

mod sync_forks;

mod unproposed_txs;

mod metering;

async fn sync_blocks_real(ex: Arc<Executor<'static>>) -> Result<()> {
    init_logger();

    // Initialize harness in testing mode
    let pow_target = 120;
    let pow_fixed_difficulty = Some(BigUint::one());
    let config = HarnessConfig {
        pow_target,
        pow_fixed_difficulty: pow_fixed_difficulty.clone(),
        confirmation_threshold: 3,
        alice_url: "tcp+tls://127.0.0.1:18340".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18341".to_string(),
    };
    let th = Harness::new(config, true, &ex).await?;

    // Generate a fork to create new blocks
    let mut fork = th.alice.validator.consensus.forks.read().await[0].full_clone()?;

    // Generate next blocks
    let block1 = th.generate_next_block(&mut fork).await?;
    let block2 = th.generate_next_block(&mut fork).await?;
    let block3 = th.generate_next_block(&mut fork).await?;
    let block4 = th.generate_next_block(&mut fork).await?;

    // Add them to nodes
    th.add_blocks(&[block1, block2.clone(), block3.clone(), block4]).await?;

    // Nodes must have one fork with 2 blocks
    th.validate_fork_chains(1, vec![2]).await;

    // Extend current fork sequence
    let block5 = th.generate_next_block(&mut fork).await?;
    // Create a new fork extending canonical
    fork = Fork::new(
        th.alice.validator.consensus.blockchain.clone(),
        th.alice.validator.consensus.module.read().await.clone(),
    )
    .await?;
    // Append block3 to fork and generate the next one
    verify_block(
        &fork.overlay,
        &fork.module,
        &mut fork.state_monotree,
        &block3,
        &block2,
        th.alice.validator.verify_fees,
    )
    .await?;
    let block6 = th.generate_next_block(&mut fork).await?;
    // Add them to nodes
    th.add_blocks(&[block5, block6]).await?;

    // Grab current best fork index
    let forks = th.alice.validator.consensus.forks.read().await;
    // If index corresponds to the small fork, confirmation
    // did not occur, as it's size is not over the threshold.
    let small_best = best_fork_index(&forks)? == 1;
    drop(forks);
    if small_best {
        // Nodes must have one fork with 3 blocks and one with 2 blocks
        th.validate_fork_chains(2, vec![3, 2]).await;
    } else {
        // Nodes must have one fork with 2 blocks and one with 1 block
        th.validate_fork_chains(2, vec![2, 1]).await;
    }

    // We are going to create a third node and try to sync from Bob
    let mut settings = Settings { localnet: true, inbound_connections: 3, ..Default::default() };
    let charlie_url = Url::parse("tcp+tls://127.0.0.1:18342")?;
    settings.inbound_addrs = vec![charlie_url];
    let bob_url = th.bob.p2p_handler.p2p.settings().read().await.inbound_addrs[0].clone();
    settings.peers = vec![bob_url];
    let charlie = generate_node(
        &th.vks,
        &th.validator_config,
        &settings,
        &ex,
        false,
        Some((block2.header.height, block2.hash())),
    )
    .await?;
    // Verify node synced
    let alice = &th.alice.validator;
    let charlie = &charlie.validator;
    assert_eq!(alice.blockchain.len(), charlie.blockchain.len());
    assert!(charlie.blockchain.headers.is_empty_sync());
    // Node must have just the best fork
    let forks = alice.consensus.forks.read().await;
    let best_fork = &forks[best_fork_index(&forks)?];
    let charlie_forks = charlie.consensus.forks.read().await;
    assert_eq!(charlie_forks.len(), 1);
    assert_eq!(charlie_forks[0].proposals.len(), best_fork.proposals.len());
    assert_eq!(charlie_forks[0].diffs.len(), best_fork.diffs.len());
    drop(forks);
    drop(charlie_forks);

    // Extend the small fork sequence and add it to nodes
    th.add_blocks(&[th.generate_next_block(&mut fork).await?]).await?;

    // Nodes must have two forks with 2 blocks each
    th.validate_fork_chains(2, vec![2, 2]).await;
    // Check charlie has the correct forks
    let charlie_forks = charlie.consensus.forks.read().await;
    if small_best {
        // If Charlie already had the small fork as its best,
        // it will have a single fork with 3 blocks.
        assert_eq!(charlie_forks.len(), 1);
        assert_eq!(charlie_forks[0].proposals.len(), 3);
        assert_eq!(charlie_forks[0].diffs.len(), 3);
    } else {
        // Charlie didn't originaly have the fork, but it
        // should be synced when its proposal was received
        assert_eq!(charlie_forks.len(), 2);
        assert_eq!(charlie_forks[0].proposals.len(), 2);
        assert_eq!(charlie_forks[0].diffs.len(), 2);
        assert_eq!(charlie_forks[1].proposals.len(), 2);
        assert_eq!(charlie_forks[1].diffs.len(), 2);
    }
    drop(charlie_forks);

    // Since the don't know if the second fork was the best,
    // we extend it until it becomes best and a confirmation
    // occurred.
    loop {
        th.add_blocks(&[th.generate_next_block(&mut fork).await?]).await?;
        // Check if confirmation occured
        if th.alice.validator.blockchain.len() > 4 {
            break
        }
    }

    // Nodes must have executed confirmation, so we validate their chains
    th.validate_chains(4 + (fork.proposals.len() - 2)).await?;
    let bob = &th.bob.validator;
    let last = alice.blockchain.last()?.1;
    assert_eq!(last, fork.proposals[fork.proposals.len() - 3]);
    assert_eq!(last, bob.blockchain.last()?.1);
    // Nodes must have one fork with 2 blocks
    th.validate_fork_chains(1, vec![2]).await;
    let last_proposal = alice.consensus.forks.read().await[0].proposals[1];
    assert_eq!(last_proposal, *fork.proposals.last().unwrap());
    assert_eq!(last_proposal, bob.consensus.forks.read().await[0].proposals[1]);

    // Same for Charlie
    charlie.confirmation().await?;
    charlie.validate_blockchain(pow_target, pow_fixed_difficulty).await?;
    assert_eq!(alice.blockchain.len(), charlie.blockchain.len());
    assert!(charlie.blockchain.headers.is_empty_sync());
    assert_eq!(last, charlie.blockchain.last()?.1);
    let charlie_forks = charlie.consensus.forks.read().await;
    assert_eq!(charlie_forks.len(), 1);
    assert_eq!(charlie_forks[0].proposals.len(), 2);
    assert_eq!(charlie_forks[0].diffs.len(), 2);
    assert_eq!(last_proposal, charlie_forks[0].proposals[1]);

    // Thanks for reading
    Ok(())
}

#[test]
fn sync_blocks() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new().each(0..4, |_| smol::block_on(ex.run(shutdown.recv()))).finish(
        || {
            smol::block_on(async {
                sync_blocks_real(ex.clone()).await.unwrap();
                drop(signal);
            })
        },
    );

    Ok(())
}

#[test]
/// Test the programmatic control of `Darkfid`.
///
/// First we initialize a daemon, start it and then perform
/// couple of restarts to verify everything works as expected.
fn darkfid_programmatic_control() -> Result<()> {
    // We check this error so we can execute same file tests in parallel,
    // otherwise second one fails to init logger here.
    if setup_test_logger(
        &[],
        false,
        Level::Info,
        //Level::Verbose,
        //Level::Debug,
        //Level::Trace
    )
    .is_err()
    {
        warn!(target: "darkfid_programmatic_control", "Logger already initialized");
    }

    // Create an executor and communication signals
    let ex = Arc::new(smol::Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new().each(0..1, |_| smol::block_on(ex.run(shutdown.recv()))).finish(
        || {
            smol::block_on(async {
                // Daemon configuration
                let mut genesis_block = darkfi::blockchain::BlockInfo::default();
                let producer_tx = genesis_block.txs.pop().unwrap();
                genesis_block.append_txs(vec![producer_tx]);
                let sled_db = sled_overlay::sled::Config::new().temporary(true).open().unwrap();
                let (_, vks) = darkfi_contract_test_harness::vks::get_cached_pks_and_vks().unwrap();
                darkfi_contract_test_harness::vks::inject(&sled_db, &vks).unwrap();
                let overlay = darkfi::blockchain::BlockchainOverlay::new(
                    &darkfi::blockchain::Blockchain::new(&sled_db).unwrap(),
                )
                .unwrap();
                darkfi::validator::utils::deploy_native_contracts(&overlay, 20).await.unwrap();
                genesis_block.header.state_root = overlay
                    .lock()
                    .unwrap()
                    .get_state_monotree()
                    .unwrap()
                    .get_headroot()
                    .unwrap()
                    .unwrap();
                let bootstrap = genesis_block.header.timestamp.inner();
                let config = darkfi::validator::ValidatorConfig {
                    confirmation_threshold: 1,
                    pow_target: 20,
                    pow_fixed_difficulty: Some(BigUint::one()),
                    genesis_block,
                    verify_fees: false,
                };
                let consensus_config = crate::ConsensusInitTaskConfig {
                    skip_sync: true,
                    checkpoint_height: None,
                    checkpoint: None,
                    bootstrap,
                };
                let rpc_settings = RpcSettings {
                    listen: Url::parse("tcp://127.0.0.1:8240").unwrap(),
                    ..RpcSettings::default()
                };

                // Initialize a daemon
                let daemon = crate::Darkfid::init(
                    &sled_db,
                    &config,
                    &darkfi::net::Settings::default(),
                    &None,
                    &ex,
                )
                .await
                .unwrap();

                // Start it
                daemon.start(&ex, &rpc_settings, &None, &consensus_config).await.unwrap();

                // Stop it
                daemon.stop().await.unwrap();

                // Start it again
                daemon.start(&ex, &rpc_settings, &None, &consensus_config).await.unwrap();

                // Stop it
                daemon.stop().await.unwrap();

                // Shutdown entirely
                drop(signal);
            })
        },
    );

    Ok(())
}
