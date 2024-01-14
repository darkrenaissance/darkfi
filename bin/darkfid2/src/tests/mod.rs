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

use std::sync::Arc;

use darkfi::{net::Settings, Result};
use darkfi_contract_test_harness::init_logger;
use smol::Executor;
use url::Url;

mod harness;
use harness::{generate_node, Harness, HarnessConfig};

mod forks;

async fn sync_pos_blocks_real(ex: Arc<Executor<'static>>) -> Result<()> {
    init_logger();

    // Initialize harness in testing mode
    let pow_target = 90;
    let pow_fixed_difficulty = None;
    let config = HarnessConfig {
        pow_target,
        pow_fixed_difficulty: pow_fixed_difficulty.clone(),
        pos_testing_mode: true,
        alice_initial: 1000,
        bob_initial: 500,
    };
    let th = Harness::new(config, false, &ex).await?;

    // Retrieve genesis block
    let previous = th.alice.validator.blockchain.last_block()?;

    // Generate next block
    let block1 = th.generate_next_pos_block(&previous, 1).await?;

    // Generate next block, with 4 empty slots inbetween
    let block2 = th.generate_next_pos_block(&block1, 5).await?;

    // Add it to nodes
    th.add_blocks(&vec![block1, block2]).await?;

    // Validate chains
    th.validate_chains(3, 7).await?;

    // We are going to create a third node and try to sync from the previous two
    let mut sync_settings =
        Settings { localnet: true, inbound_connections: 3, ..Default::default() };

    let charlie_url = Url::parse("tcp+tls://127.0.0.1:18342")?;
    sync_settings.inbound_addrs = vec![charlie_url];
    let alice_url = th.alice.sync_p2p.settings().inbound_addrs[0].clone();
    let bob_url = th.bob.sync_p2p.settings().inbound_addrs[0].clone();
    sync_settings.peers = vec![alice_url, bob_url];
    let charlie =
        generate_node(&th.vks, &th.validator_config, &sync_settings, None, &ex, false).await?;
    // Verify node synced
    let genesis_txs_total = th.config.alice_initial + th.config.bob_initial;
    let alice = &th.alice.validator;
    let charlie = &charlie.validator;
    charlie
        .validate_blockchain(genesis_txs_total, vec![], pow_target, pow_fixed_difficulty)
        .await?;
    assert_eq!(alice.blockchain.len(), charlie.blockchain.len());
    assert_eq!(alice.blockchain.slots.len(), charlie.blockchain.slots.len());

    // Thanks for reading
    Ok(())
}

#[test]
fn sync_pos_blocks() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new().each(0..4, |_| smol::block_on(ex.run(shutdown.recv()))).finish(
        || {
            smol::block_on(async {
                sync_pos_blocks_real(ex.clone()).await.unwrap();
                drop(signal);
            })
        },
    );

    Ok(())
}
