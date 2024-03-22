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

use std::sync::Arc;

use darkfi::{net::Settings, validator::utils::best_fork_index, Result};
use darkfi_contract_test_harness::init_logger;
use darkfi_sdk::num_traits::One;
use num_bigint::BigUint;
use smol::Executor;
use url::Url;

mod harness;
use harness::{generate_node, Harness, HarnessConfig};

mod forks;

mod sync_forks;

async fn sync_blocks_real(ex: Arc<Executor<'static>>) -> Result<()> {
    init_logger();

    // Initialize harness in testing mode
    let pow_target = 90;
    let pow_fixed_difficulty = Some(BigUint::one());
    let config = HarnessConfig {
        pow_target,
        pow_fixed_difficulty: pow_fixed_difficulty.clone(),
        finalization_threshold: 3,
        alice_initial: 1000,
        bob_initial: 500,
    };
    let th = Harness::new(config, true, &ex).await?;

    // Retrieve genesis block
    let genesis = th.alice.validator.blockchain.last_block()?;

    // Generate next blocks
    let block1 = th.generate_next_block(&genesis).await?;
    let block2 = th.generate_next_block(&block1).await?;
    let block3 = th.generate_next_block(&block2).await?;
    let block4 = th.generate_next_block(&block3).await?;

    // Add them to nodes
    th.add_blocks(&vec![block1, block2, block3.clone(), block4.clone()]).await?;

    // Nodes must have one fork with 2 blocks
    th.validate_fork_chains(1, vec![2]).await;

    // Extend current fork sequence
    let block5 = th.generate_next_block(&block4).await?;
    // Create a new fork extending canonical
    let block6 = th.generate_next_block(&block3).await?;
    // Add them to nodes
    th.add_blocks(&vec![block5, block6.clone()]).await?;

    // Grab current best fork index
    let forks = th.alice.validator.consensus.forks.read().await;
    // If index corresponds to the small fork, finalization
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
    let bob_url = th.bob.p2p.settings().inbound_addrs[0].clone();
    settings.peers = vec![bob_url];
    let charlie =
        generate_node(&th.vks, &th.validator_config, &settings, &ex, false, false).await?;
    // Verify node synced
    let alice = &th.alice.validator;
    let charlie = &charlie.validator;
    charlie.validate_blockchain(pow_target, pow_fixed_difficulty.clone()).await?;
    assert_eq!(alice.blockchain.len(), charlie.blockchain.len());
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
    let block7 = th.generate_next_block(&block6).await?;
    th.add_blocks(&vec![block7.clone()]).await?;

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
    // we extend it until it becomes best and a finalization
    // occurred.
    let mut fork_sequence = vec![block6, block7];
    loop {
        let proposal = th.generate_next_block(fork_sequence.last().unwrap()).await?;
        th.add_blocks(&vec![proposal.clone()]).await?;
        fork_sequence.push(proposal);
        // Check if finalization occured
        if th.alice.validator.blockchain.len() > 4 {
            break
        }
    }

    // Nodes must have executed finalization, so we validate their chains
    th.validate_chains(4 + (fork_sequence.len() - 2)).await?;
    let bob = &th.bob.validator;
    let last = alice.blockchain.last()?.1;
    assert_eq!(last, fork_sequence[fork_sequence.len() - 3].hash()?);
    assert_eq!(last, bob.blockchain.last()?.1);
    // Nodes must have one fork with 2 blocks
    th.validate_fork_chains(1, vec![2]).await;
    let last_proposal = alice.consensus.forks.read().await[0].proposals[1];
    assert_eq!(last_proposal, fork_sequence.last().unwrap().hash()?);
    assert_eq!(last_proposal, bob.consensus.forks.read().await[0].proposals[1]);

    // Same for Charlie
    charlie.finalization().await?;
    charlie.validate_blockchain(pow_target, pow_fixed_difficulty).await?;
    assert_eq!(alice.blockchain.len(), charlie.blockchain.len());
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
