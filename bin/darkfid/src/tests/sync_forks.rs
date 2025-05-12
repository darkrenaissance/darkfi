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

use darkfi::{net::Settings, validator::utils::best_fork_index, Result};
use darkfi_contract_test_harness::init_logger;
use darkfi_sdk::num_traits::One;
use num_bigint::BigUint;
use smol::Executor;
use url::Url;

use crate::tests::{generate_node, Harness, HarnessConfig};

async fn sync_forks_real(ex: Arc<Executor<'static>>) -> Result<()> {
    init_logger();

    // Initialize harness in testing mode
    let pow_target = 120;
    let pow_fixed_difficulty = Some(BigUint::one());
    let config = HarnessConfig {
        pow_target,
        pow_fixed_difficulty: pow_fixed_difficulty.clone(),
        confirmation_threshold: 6,
        alice_url: "tcp+tls://127.0.0.1:18440".to_string(),
        bob_url: "tcp+tls://127.0.0.1:18441".to_string(),
    };
    let th = Harness::new(config, true, &ex).await?;

    // Generate 3 forks
    let mut fork0 = th.alice.validator.consensus.forks.read().await[0].full_clone()?;
    let mut fork1 = fork0.full_clone()?;
    let mut fork2 = fork1.full_clone()?;

    // Extend first fork with 3 blocks
    th.add_blocks(&vec![
        th.generate_next_block(&mut fork0).await?,
        th.generate_next_block(&mut fork0).await?,
        th.generate_next_block(&mut fork0).await?,
    ])
    .await?;

    // Extend second fork with 1 block
    th.add_blocks(&vec![th.generate_next_block(&mut fork1).await?]).await?;

    // Extend third fork with 1 block
    th.add_blocks(&vec![th.generate_next_block(&mut fork2).await?]).await?;

    // Check nodes have all the forks
    th.validate_fork_chains(3, vec![3, 1, 1]).await;

    // We are going to create a third node and try to sync from Bob
    let mut settings = Settings { localnet: true, inbound_connections: 3, ..Default::default() };

    let charlie_url = Url::parse("tcp+tls://127.0.0.1:18442")?;
    settings.inbound_addrs = vec![charlie_url];
    let bob_url = th.bob.p2p_handler.p2p.settings().read().await.inbound_addrs[0].clone();
    settings.peers = vec![bob_url];
    let charlie = generate_node(&th.vks, &th.validator_config, &settings, &ex, false, None).await?;

    // Verify node synced the best fork
    let forks = th.alice.validator.consensus.forks.read().await;
    let best_fork = &forks[best_fork_index(&forks)?];
    let charlie_forks = charlie.validator.consensus.forks.read().await;
    assert_eq!(charlie_forks.len(), 1);
    assert_eq!(charlie_forks[0].proposals.len(), best_fork.proposals.len());
    let small_best = best_fork.proposals.len() == 1;
    drop(forks);
    drop(charlie_forks);

    // Extend the small fork sequences and add it to nodes
    th.add_blocks(&vec![th.generate_next_block(&mut fork1).await?]).await?;
    th.add_blocks(&vec![th.generate_next_block(&mut fork2).await?]).await?;

    // Check charlie has the correct forks
    let charlie_forks = charlie.validator.consensus.forks.read().await;
    if small_best {
        // If Charlie already had a small fork as its best,
        // it will have two forks with 2 blocks each.
        assert_eq!(charlie_forks.len(), 2);
        assert_eq!(charlie_forks[0].proposals.len(), 2);
        assert_eq!(charlie_forks[1].proposals.len(), 2);
    } else {
        // Charlie didn't originaly have the forks, but they
        // should be synced when their proposals were received
        assert_eq!(charlie_forks.len(), 3);
        assert_eq!(charlie_forks[0].proposals.len(), 3);
        assert_eq!(charlie_forks[1].proposals.len(), 2);
        assert_eq!(charlie_forks[2].proposals.len(), 2);
    }
    drop(charlie_forks);

    // Thanks for reading
    Ok(())
}

#[test]
fn sync_forks() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new().each(0..4, |_| smol::block_on(ex.run(shutdown.recv()))).finish(
        || {
            smol::block_on(async {
                sync_forks_real(ex.clone()).await.unwrap();
                drop(signal);
            })
        },
    );

    Ok(())
}
