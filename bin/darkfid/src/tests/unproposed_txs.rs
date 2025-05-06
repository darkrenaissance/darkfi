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

//! Test cases for unproprosed transactions.
//!
//! The following are supported test cases:
//! - Verifying the processing of unproposed transactions that are within the unproposed transactions gas limit.
//! - Verifying the processing of unproposed transactions that exceed the unproposed transactions gas limit.
//!
//! The tests were written with a 'GAS_LIMIT_UNPROPOSED_TXS' set to `23_822_290 * 50`. The number `23_822_290` is derived
//! from the average gas used per transaction, yielding an overall limit of 1_191_114_500 for the pool
//! of unproposed transactions.
//!
//! Please update the test to reflect any changes to the unproposed transactions gas limit value.

use darkfi::Result;
use std::sync::Arc;

use crate::tests::{Harness, HarnessConfig};
use darkfi::validator::{consensus::GAS_LIMIT_UNPROPOSED_TXS, utils::best_fork_index};
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use darkfi_sdk::{crypto::BaseBlind, num_traits::One};
use num_bigint::BigUint;
use rand::rngs::OsRng;
use smol::Executor;

/// Simulates the processing of a specified number of unproposed transactions, returning
/// the total number of unproposed transactions and gas used.
async fn simulate_unproposed_txs(
    num_txs: u64,
    alice_url: String,
    bob_url: String,
    ex: Arc<Executor<'static>>,
) -> Result<(u64, u64)> {
    init_logger();

    // Set current block height used to create and retrieve unproposed transactions
    let current_block_height = 1;

    // Create chain test harness configuration
    let pow_target = 90;
    let pow_fixed_difficulty = Some(BigUint::one());
    let config = HarnessConfig {
        pow_target,
        pow_fixed_difficulty: pow_fixed_difficulty.clone(),
        confirmation_threshold: 6,
        alice_url,
        bob_url,
    };

    // Create chain test harness using created configuration
    let blockchain_test_harness = Harness::new(config, false, &ex).await?;

    // Get validator and generate the fork
    let validator = blockchain_test_harness.alice.validator.clone();
    validator.consensus.generate_empty_fork().await?;

    // Create contract test harness
    const HOLDERS: [Holder; 1] = [Holder::Alice];
    let mut contract_test_harness = TestHarness::new(&HOLDERS, false).await?;

    // Create and add pending transactions
    for counter in 0..num_txs {
        let (tx, _, _, _) = contract_test_harness
            .token_mint(
                counter + 1,
                &Holder::Alice,
                &Holder::Alice,
                BaseBlind::random(&mut OsRng),
                None,
                None,
                current_block_height,
            )
            .await?;
        validator.append_tx(&tx, true).await?;
    }

    // Obtain fork
    let forks = validator.consensus.forks.read().await;
    let best_fork = &forks[best_fork_index(&forks)?];

    // Retrieve unproposed transactions
    let (tx, total_gas_used, _, _) = best_fork
        .unproposed_txs(
            &best_fork.clone().blockchain,
            current_block_height,
            validator.consensus.module.read().await.target,
            false,
        )
        .await?;

    Ok((tx.len() as u64, total_gas_used))
}

/// Tests the processing of unproposed transactions within `GAS_LIMIT_UNPROPOSED_TXS`.
///
/// Note: In this test scenario, the mempool is populated with 5 pending transactions that each use roughly 9_851_908 gas,
/// falling within `GAS_LIMIT_UNPROPOSED_TXS`.
#[test]
fn test_unproposed_txs_within_gas_limit() -> Result<()> {
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new().each(0..1, |_| smol::block_on(ex.run(shutdown.recv()))).finish(
        || {
            smol::block_on(async {
                // Receive number of unproposed txs within gas limit
                let (num_unproposed_txs, _) = simulate_unproposed_txs(
                    5,
                    "tcp+tls://127.0.0.1:18540".to_string(),
                    "tcp+tls://127.0.0.1:18541".to_string(),
                    ex.clone(),
                )
                .await
                .unwrap();

                // Shutdown spawned nodes
                signal.send(()).await.unwrap();

                // Verify test result
                assert_eq!(num_unproposed_txs, 5);
            });
        },
    );

    Ok(())
}

/// Tests the processing of unproposed transactions with a mempool of transactions that collectively exceed `GAS_LIMIT_UNPROPOSED_TXS`.
///
/// Note: In this test scenario, the mempool is populated with 135 pending transactions, with an average gas usage of 9_851_647 gas.
/// The total estimated gas usage of these transactions exceeds `GAS_LIMIT_UNPROPOSED_TXS`.
#[test]
fn test_unproposed_txs_exceeding_gas_limit() -> Result<()> {
    let avg_gas_usage = 9_851_647;
    let min_expected = GAS_LIMIT_UNPROPOSED_TXS / avg_gas_usage;
    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    easy_parallel::Parallel::new().each(0..1, |_| smol::block_on(ex.run(shutdown.recv()))).finish(
        || {
            smol::block_on(async {
                // Receive total gas used by simulating a number of transactions that will exceed gas limit
                let (num_unproposed_txs, total_gas_used) = simulate_unproposed_txs(
                    135,
                    "tcp+tls://127.0.0.1:18640".to_string(),
                    "tcp+tls://127.0.0.1:18641".to_string(),
                    ex.clone(),
                )
                .await
                .unwrap();

                // Shutdown spawned nodes
                signal.send(()).await.unwrap();

                // Verify min expected test result
                assert!(num_unproposed_txs >= min_expected);

                // Verify test result falls within gas limit
                assert!(total_gas_used <= GAS_LIMIT_UNPROPOSED_TXS);
            });
        },
    );

    Ok(())
}
