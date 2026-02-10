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

//! Integration test for payments between Alice and Bob.
//!
//! We first mint them different tokens, and then they send them to each
//! other a couple of times.
//!
//! With this test, we want to confirm the money contract transfer state
//! transitions work between multiple parties and are able to be verified.
//! We also test atomic swaps with some of the coins that have been produced.
//!
//! TODO: Malicious cases

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use tracing::info;

#[test]
fn mint_pay_swap() -> Result<()> {
    smol::block_on(async {
        init_logger();

        use Holder::{Alice, Bob};

        const ALICE_INITIAL: u64 = 100;
        const BOB_INITIAL: u64 = 200;
        const ALICE_SEND: u64 = 50;
        const BOB_SEND: u64 = 180;
        let block_height = 0;

        let mut th = TestHarness::new(&[Alice, Bob], false).await?;

        // Mint tokens for Alice and Bob
        info!(target: "money", "Minting tokens for Alice and Bob");
        let alice_token = th.token_mint_to_all(ALICE_INITIAL, &Alice, &Alice, block_height).await?;
        let bob_token = th.token_mint_to_all(BOB_INITIAL, &Bob, &Bob, block_height).await?;

        // Alice sends some tokens to Bob
        info!(target: "money", "Alice sends {ALICE_SEND} to Bob");
        th.transfer_to_all(ALICE_SEND, &Alice, &Bob, alice_token, block_height).await?;

        assert_eq!(th.coins(&Alice).len(), 1); // change coin
        assert_eq!(th.coins(&Bob).len(), 2); // original BOB + received ALICE
        assert_eq!(th.balance(&Alice, alice_token), ALICE_INITIAL - ALICE_SEND);
        assert_eq!(th.balance(&Bob, alice_token), ALICE_SEND);

        // Bob sends some tokens to Alice
        info!(target: "money", "Bob sends {BOB_SEND} to Alice");
        th.transfer_to_all(BOB_SEND, &Bob, &Alice, bob_token, block_height).await?;

        assert_eq!(th.balance(&Alice, alice_token), ALICE_INITIAL - ALICE_SEND);
        assert_eq!(th.balance(&Alice, bob_token), BOB_SEND);
        assert_eq!(th.balance(&Bob, alice_token), ALICE_SEND);
        assert_eq!(th.balance(&Bob, bob_token), BOB_INITIAL - BOB_SEND);

        // Alice and Bob swap back their foreign tokens
        info!(target: "money", "Alice and Bob swap foreign tokens");
        let alice_bob_coin = th.coins_by_token(&Alice, bob_token)[0].clone();
        let bob_alice_coin = th.coins_by_token(&Bob, alice_token)[0].clone();
        th.otc_swap_to_all(&Alice, &alice_bob_coin, &Bob, &bob_alice_coin, block_height).await?;

        // Both now hold only their own token types
        assert!(th.coins(&Alice).iter().all(|c| c.note.token_id == alice_token));
        assert!(th.coins(&Bob).iter().all(|c| c.note.token_id == bob_token));

        // Consolidate fragmented coins
        info!(target: "money", "Consolidating coins");
        th.consolidate_to_all(&Alice, alice_token, block_height).await?;
        th.consolidate_to_all(&Bob, bob_token, block_height).await?;

        assert_eq!(th.coins(&Alice).len(), 1);
        assert_eq!(th.coins(&Alice)[0].note.value, ALICE_INITIAL);
        assert_eq!(th.coins(&Bob).len(), 1);
        assert_eq!(th.coins(&Bob)[0].note.value, BOB_INITIAL);

        // Final swap: Alice and Bob exchange everything
        info!(target: "money", "Final swap: Alice and Bob exchange all tokens");
        let alice_coin = th.coins(&Alice)[0].clone();
        let bob_coin = th.coins(&Bob)[0].clone();
        th.otc_swap_to_all(&Alice, &alice_coin, &Bob, &bob_coin, block_height).await?;

        assert_eq!(th.balance(&Alice, bob_token), BOB_INITIAL);
        assert_eq!(th.balance(&Bob, alice_token), ALICE_INITIAL);
        assert_eq!(th.coins(&Alice).len(), 1);
        assert_eq!(th.coins(&Bob).len(), 1);

        // Thanks for reading
        Ok(())
    })
}
