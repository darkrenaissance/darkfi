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

use std::{env, str::FromStr};

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use log::info;
use rand::{prelude::IteratorRandom, Rng};

#[async_std::test]
async fn alice2alice_random_amounts() -> Result<()> {
    init_logger();

    // Holders this test will use
    const HOLDERS: [Holder; 2] = [Holder::Faucet, Holder::Alice];

    const ALICE_AIRDROP: u64 = 1000;

    // Slot to verify against
    let current_slot = 0;

    // n transactions to loop
    let mut n = 3;
    for arg in env::args() {
        match usize::from_str(&arg) {
            Ok(v) => {
                n = v;
                break
            }
            Err(_) => continue,
        };
    }

    // Initialize harness
    let mut th = TestHarness::new(&["money".to_string()]).await?;

    info!(target: "money", "[Faucet] ========================");
    info!(target: "money", "[Faucet] Building Alice's airdrop");
    info!(target: "money", "[Faucet] ========================");
    let (airdrop_tx, airdrop_params) =
        th.airdrop_native(ALICE_AIRDROP, &Holder::Alice, None, None, None, None)?;

    for holder in &HOLDERS {
        info!(target: "money", "[{holder:?}] ==========================");
        info!(target: "money", "[{holder:?}] Executing Alice airdrop tx");
        info!(target: "money", "[{holder:?}] ==========================");
        th.execute_airdrop_native_tx(holder, &airdrop_tx, &airdrop_params, current_slot).await?;
    }

    th.assert_trees(&HOLDERS);

    // Gather new owncoins
    let mut owncoins = vec![];
    let owncoin = th.gather_owncoin(&Holder::Alice, &airdrop_params.outputs[0], None)?;
    let token_id = owncoin.note.token_id;
    owncoins.push(owncoin);

    // Execute transactions loop
    for i in 0..n {
        info!(target: "money", "[Alice] ===============================================");
        info!(target: "money", "[Alice] Building Money::Transfer params for transfer {}", i);
        info!(target: "money", "[Alice] Alice coins: {}", owncoins.len());
        for (i, c) in owncoins.iter().enumerate() {
            info!(target: "money", "[Alice] \t coin {} value: {}", i, c.note.value);
        }
        let amount = rand::thread_rng().gen_range(1..ALICE_AIRDROP);
        info!(target: "money", "[Alice] Sending: {}", amount);
        info!(target: "money", "[Alice] ===============================================");
        let (tx, params, spent_coins) =
            th.transfer(amount, &Holder::Alice, &Holder::Alice, &owncoins, token_id)?;

        // Remove the owncoins we've spent
        for spent in spent_coins {
            owncoins.retain(|x| x != &spent);
        }

        // Verify transaction
        info!(target: "money", "[Faucet] ================================");
        info!(target: "money", "[Faucet] Executing Alice2Alice payment tx");
        info!(target: "money", "[Faucet] ================================");
        th.execute_transfer_tx(&Holder::Faucet, &tx, &params, current_slot, true).await?;

        info!(target: "money", "[Alice] ================================");
        info!(target: "money", "[Alice] Executing Alice2Alice payment tx");
        info!(target: "money", "[Alice] ================================");
        th.execute_transfer_tx(&Holder::Alice, &tx, &params, current_slot, false).await?;

        // Gather new owncoins
        owncoins.append(&mut th.gather_multiple_owncoins(&Holder::Alice, &params.outputs)?);

        th.assert_trees(&HOLDERS);
    }

    // Statistics
    th.statistics();

    // Thanks for reading
    Ok(())
}

#[async_std::test]
async fn alice2alice_multiplecoins_random_amounts() -> Result<()> {
    init_logger();

    // Holders this test will use
    const HOLDERS: [Holder; 2] = [Holder::Faucet, Holder::Alice];

    // Slot to verify against
    let current_slot = 0;

    // N blocks to simulate
    let mut n = 3;
    for arg in env::args() {
        match usize::from_str(&arg) {
            Ok(v) => {
                n = v;
                break
            }
            Err(_) => continue,
        };
    }

    // Initialize harness
    let mut th = TestHarness::new(&["money".to_string()]).await?;

    // Mint 10 coins
    let mut token_ids = vec![];
    let mut minted_amounts = vec![];
    let mut owncoins = vec![];
    for i in 0..10 {
        let amount = rand::thread_rng().gen_range(2..1000);
        info!(target: "money", "[Faucet] ===================================================");
        info!(target: "money", "[Faucet] Building Money::Mint params for Alice's mint for token {} and amount {}", i, amount);
        info!(target: "money", "[Faucet] ===================================================");
        let (mint_tx, mint_params) =
            th.token_mint(amount, &Holder::Alice, &Holder::Alice, None, None)?;

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] =======================");
            info!(target: "money", "[{holder:?}] Executing Alice mint tx");
            info!(target: "money", "[{holder:?}] =======================");
            th.execute_token_mint_tx(holder, &mint_tx, &mint_params, current_slot).await?;
        }

        th.assert_trees(&HOLDERS);

        // Gather new owncoins
        let owncoin = th.gather_owncoin(&Holder::Alice, &mint_params.output, None)?;
        let token_id = owncoin.note.token_id;
        owncoins.push(vec![owncoin]);
        minted_amounts.push(amount);
        token_ids.push(token_id);
    }

    // Simulating N blocks
    for b in 0..n {
        // Get a random sized sample of owncoins
        let sample =
            (0..10).choose_multiple(&mut rand::thread_rng(), rand::thread_rng().gen_range(1..10));
        info!(target: "money", "[Alice] =====================================");
        info!(target: "money", "[Alice] Generating transactions for block: {}", b);
        info!(target: "money", "[Alice] Coins to use: {:?}", sample);
        info!(target: "money", "[Alice] =====================================");

        // Generate a transaction for each coin
        let mut txs = vec![];
        let mut txs_params = vec![];
        for index in sample {
            info!(target: "money", "[Alice] ===============================================");
            info!(target: "money", "[Alice] Building Money::Transfer params for coin {}", index);
            let mut coins = owncoins[index].clone();
            let token_id = token_ids[index];
            let mint_amount = minted_amounts[index];
            info!(target: "money", "[Alice] Alice coins: {}", coins.len());
            for (i, c) in coins.iter().enumerate() {
                info!(target: "money", "[Alice] \t coin {} value: {}", i, c.note.value);
            }
            let amount = rand::thread_rng().gen_range(1..mint_amount);
            info!(target: "money", "[Alice] Sending: {}", amount);
            info!(target: "money", "[Alice] ===============================================");
            let (tx, params, spent_coins) =
                th.transfer(amount, &Holder::Alice, &Holder::Alice, &coins, token_id)?;

            // Remove the owncoins we've spent
            for spent in spent_coins {
                coins.retain(|x| x != &spent);
            }

            // Gather new owncoins
            coins.append(&mut th.gather_multiple_owncoins(&Holder::Alice, &params.outputs)?);

            // Store transaction and its params
            txs.push(tx);
            txs_params.push(params);

            // Replace coins
            owncoins[index] = coins;
        }

        info!(target: "money", "[Faucet] =================================");
        info!(target: "money", "[Faucet] Executing Alice2Alice payment txs");
        info!(target: "money", "[Faucet] =================================");
        th.execute_multiple_transfer_txs(&Holder::Faucet, &txs, &txs_params, current_slot, true)
            .await?;

        info!(target: "money", "[Alice] =================================");
        info!(target: "money", "[Alice] Executing Alice2Alice payment txs");
        info!(target: "money", "[Alice] =================================");
        th.execute_multiple_transfer_txs(&Holder::Alice, &txs, &txs_params, current_slot, false)
            .await?;

        th.assert_trees(&HOLDERS);
    }

    // Statistics
    th.statistics();

    // Thanks for reading
    Ok(())
}
