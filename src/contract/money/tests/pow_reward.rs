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

//! Test for PoW reward transaction verification correctness.
//!
//! We first reward Alice some native tokens, and then she send some of them to Bob.
//!
//! With this test, we want to confirm the PoW reward transactions execution works
//! and generated tokens can be processed as usual between multiple parties,
//! with detection of erroneous transactions.

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness, TxAction};
use darkfi_sdk::{blockchain::pow_expected_reward, crypto::DARK_TOKEN_ID};
use log::info;

#[test]
fn pow_reward() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use
        const HOLDERS: [Holder; 3] = [Holder::Faucet, Holder::Alice, Holder::Bob];

        // Slot to verify against
        let mut current_slot = 0;

        // Initialize harness
        let mut th = TestHarness::new(&["money".to_string()]).await?;

        let mut alice_owncoins = vec![];
        let mut bob_owncoins = vec![];

        // We are going to generate some erroneous transactions to
        // test some malicious cases.
        info!(target: "money", "[Malicious] =======================================");
        info!(target: "money", "[Malicious] Building PoW reward tx for genesis slot");
        info!(target: "money", "[Malicious] =======================================");
        let (pow_reward_tx, _) = th.pow_reward(&Holder::Alice, current_slot, Some(0))?;

        info!(target: "money", "[Malicious] =======================================");
        info!(target: "money", "[Malicious] Checking PoW reward tx for genesis slot");
        info!(target: "money", "[Malicious] =======================================");
        th.execute_erroneous_txs(
            TxAction::MoneyPoWReward,
            &Holder::Alice,
            &[pow_reward_tx.clone()],
            current_slot,
            1,
        )
        .await?;

        current_slot += 1;
        th.generate_slot(current_slot).await?;

        let alice_reward = pow_expected_reward(current_slot);
        info!(target: "money", "[Malicious] ================================");
        info!(target: "money", "[Malicious] Building erroneous PoW reward tx");
        info!(target: "money", "[Malicious] ================================");
        let (pow_reward_tx, _) =
            th.pow_reward(&Holder::Alice, current_slot, Some(alice_reward + 1))?;

        info!(target: "money", "[Malicious] =======================================");
        info!(target: "money", "[Malicious] Checking erroneous amount PoW reward tx");
        info!(target: "money", "[Malicious] =======================================");
        th.execute_erroneous_txs(
            TxAction::MoneyPoWReward,
            &Holder::Alice,
            &[pow_reward_tx.clone()],
            current_slot,
            1,
        )
        .await?;

        info!(target: "money", "[Alice] ======================");
        info!(target: "money", "[Alice] Building PoW reward tx");
        info!(target: "money", "[Alice] ======================");
        let (pow_reward_tx, pow_reward_params) =
            th.pow_reward(&Holder::Alice, current_slot, None)?;

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] =============================");
            info!(target: "money", "[{holder:?}] Executing Alice PoW reward tx");
            info!(target: "money", "[{holder:?}] =============================");
            th.execute_pow_reward_tx(holder, &pow_reward_tx, &pow_reward_params, current_slot)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Alice gathers her new owncoin
        let alice_oc = th.gather_owncoin(&Holder::Alice, &pow_reward_params.output, None)?;
        alice_owncoins.push(alice_oc);

        // Now Alice can send a little bit of funds to Bob
        let alice_send = alice_reward / 2;
        info!(target: "money", "[Alice] ====================================================");
        info!(target: "money", "[Alice] Building Money::Transfer params for a payment to Bob");
        info!(target: "money", "[Alice] ====================================================");
        let (transfer_tx, transfer_params, spent_coins) =
            th.transfer(alice_send, &Holder::Alice, &Holder::Bob, &alice_owncoins, *DARK_TOKEN_ID)?;

        // Validating transfer params
        assert!(transfer_params.inputs.len() == 1);
        assert!(transfer_params.outputs.len() == 2);
        assert!(spent_coins.len() == 1);
        alice_owncoins.retain(|x| x != &spent_coins[0]);
        assert!(alice_owncoins.is_empty());

        for holder in &HOLDERS {
            info!(target: "money", "[{holder:?}] ==============================");
            info!(target: "money", "[{holder:?}] Executing Alice2Bob payment tx");
            info!(target: "money", "[{holder:?}] ==============================");
            th.execute_transfer_tx(holder, &transfer_tx, &transfer_params, current_slot, true)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Alice should now have one OwnCoin with the change from the above transaction.
        let alice_oc = th.gather_owncoin(&Holder::Alice, &transfer_params.outputs[0], None)?;
        alice_owncoins.push(alice_oc);

        // Bob should have this new one.
        let bob_oc = th.gather_owncoin(&Holder::Bob, &transfer_params.outputs[1], None)?;
        bob_owncoins.push(bob_oc);

        // Validating transaction outcomes
        assert!(alice_owncoins.len() == 1);
        assert!(bob_owncoins.len() == 1);
        assert!(alice_owncoins[0].note.value == alice_reward - alice_send);
        assert!(bob_owncoins[0].note.value == alice_send);

        // Statistics
        th.statistics();

        // Thanks for reading
        Ok(())
    })
}
