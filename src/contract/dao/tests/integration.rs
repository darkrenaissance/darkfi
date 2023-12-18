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

use darkfi::Result;
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use darkfi_dao_contract::{
    client::DaoVoteNote,
    model::{Dao, DaoBlindAggregateVote},
};
use darkfi_money_contract::model::CoinParams;
use darkfi_sdk::{
    crypto::{
        pasta_prelude::Field, pedersen_commitment_u64, poseidon_hash, DAO_CONTRACT_ID,
        DARK_TOKEN_ID,
    },
    pasta::pallas,
};
use log::info;
use rand::rngs::OsRng;

#[test]
fn integration_test() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Holders this test will use:
        // * Faucet airdrops DRK
        // * Alice, Bob, and Charlie are members of the DAO.
        // * Rachel is the proposal recipient.
        // * Dao is the DAO wallet
        const HOLDERS: [Holder; 6] = [
            Holder::Faucet,
            Holder::Alice,
            Holder::Bob,
            Holder::Charlie,
            Holder::Rachel,
            Holder::Dao,
        ];

        // Initialize harness
        let mut th = TestHarness::new(&["money".to_string(), "dao".to_string()]).await?;

        // We'll use the ALICE token as the DAO governance token
        let gov_token_id = th.token_id(&Holder::Alice);
        const ALICE_GOV_SUPPLY: u64 = 100_000_000;
        const BOB_GOV_SUPPLY: u64 = 100_000_000;
        const CHARLIE_GOV_SUPPLY: u64 = 100_000_000;
        // And the DRK token as the treasury token
        let drk_token_id = *DARK_TOKEN_ID;
        const DRK_TOKEN_SUPPLY: u64 = 1_000_000_000;
        // The tokens we want to send via the proposal
        const PROPOSAL_AMOUNT: u64 = 250_000_000;

        // Slot to verify against
        let current_slot = 0;

        // DAO parameters
        let dao_keypair = th.holders.get(&Holder::Dao).unwrap().keypair;
        let dao = Dao {
            proposer_limit: 100_000_000,
            quorum: 199_999_999,
            approval_ratio_base: 2,
            approval_ratio_quot: 1,
            gov_token_id,
            public_key: dao_keypair.public,
            bulla_blind: pallas::Base::random(&mut OsRng),
        };

        // ====================
        // Dao::Mint
        // Create the DAO bulla
        // ====================
        info!("Stage 1. Creating DAO bulla");

        info!("[Dao] Building DAO mint tx");
        let (dao_mint_tx, dao_mint_params) = th.dao_mint(&dao, &dao_keypair)?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing DAO Mint tx");
            th.execute_dao_mint_tx(holder, &dao_mint_tx, &dao_mint_params, current_slot).await?;
        }

        th.assert_trees(&HOLDERS);

        // =======================================
        // Airdrop some treasury tokens to the DAO
        // =======================================
        info!("Stage 2. Send Treasury token");

        info!("[Faucet] Building DAO airdrop tx");
        let (airdrop_tx, airdrop_params) = th.airdrop_native(
            DRK_TOKEN_SUPPLY,
            &Holder::Dao,
            Some(DAO_CONTRACT_ID.inner()),           // spend_hook
            Some(dao_mint_params.dao_bulla.inner()), // user_data
        )?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing DAO airdrop tx");
            th.execute_airdrop_native_tx(holder, &airdrop_tx, &airdrop_params, current_slot)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Gather the DAO owncoin
        th.gather_owncoin(&Holder::Dao, &airdrop_params.outputs[0], None)?;

        // ======================================
        // Mint the governance token to 3 holders
        // ======================================
        info!("Stage 3. Minting governance token");

        info!("[Alice] Building governance token mint tx for Alice");
        let (a_token_mint_tx, a_token_mint_params) =
            th.token_mint(ALICE_GOV_SUPPLY, &Holder::Alice, &Holder::Alice, None, None)?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing governance token mint tx for Alice");
            th.execute_token_mint_tx(holder, &a_token_mint_tx, &a_token_mint_params, current_slot)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Gather owncoin
        th.gather_owncoin(&Holder::Alice, &a_token_mint_params.output, None)?;

        info!("[Alice] Building governance token mint tx for Bob");
        let (b_token_mint_tx, b_token_mint_params) =
            th.token_mint(BOB_GOV_SUPPLY, &Holder::Alice, &Holder::Bob, None, None)?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing governance token mint tx for Bob");
            th.execute_token_mint_tx(holder, &b_token_mint_tx, &b_token_mint_params, current_slot)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Gather owncoin
        th.gather_owncoin(&Holder::Bob, &b_token_mint_params.output, None)?;

        info!("[Alice] Building governance token mint tx for Charlie");
        let (c_token_mint_tx, c_token_mint_params) =
            th.token_mint(CHARLIE_GOV_SUPPLY, &Holder::Alice, &Holder::Charlie, None, None)?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing governance token mint tx for Charlie");
            th.execute_token_mint_tx(holder, &c_token_mint_tx, &c_token_mint_params, current_slot)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Gather owncoin
        th.gather_owncoin(&Holder::Charlie, &c_token_mint_params.output, None)?;

        // ================
        // Dao::Propose
        // Propose the vote
        // ================
        info!("Stage 4. Propose the vote");
        // TODO: look into proposal expiry once time for voting has finished
        // TODO: Is it possible for an invalid transfer() to be constructed on exec()?
        //       Need to look into this.
        info!("[Alice] Building DAO proposal tx");

        // These coins are passed around to all DAO members who verify its validity
        // They also check hashing them equals the proposal_commit
        let coins = vec![CoinParams {
            public_key: th.holders.get(&Holder::Rachel).unwrap().keypair.public,
            value: PROPOSAL_AMOUNT,
            token_id: drk_token_id,
            serial: pallas::Base::random(&mut OsRng),
            spend_hook: pallas::Base::ZERO,
            user_data: pallas::Base::ZERO,
        }];
        // We can add whatever we want in here, even arbitrary text
        // It's up to the auth module to decide what to do with it.
        let content_commit = poseidon_hash([coins[0].to_coin().inner()]);
        let auth_contract_id = pallas::Base::ZERO;
        let auth_function_id = pallas::Base::ZERO;

        let (propose_tx, propose_params, propose_info) = th.dao_propose(
            &Holder::Alice,
            content_commit,
            auth_contract_id,
            auth_function_id,
            &Holder::Rachel,
            PROPOSAL_AMOUNT,
            drk_token_id,
            &dao,
            &dao_mint_params.dao_bulla,
        )?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing DAO proposal tx");
            th.execute_dao_propose_tx(holder, &propose_tx, &propose_params, current_slot).await?;
        }

        th.assert_trees(&HOLDERS);

        // =====================================
        // Dao::Vote
        // Proposal is accepted. Start the vote.
        // =====================================
        info!("Stage 5. Start voting");

        info!("[Alice] Building vote tx (yes)");
        let (alice_vote_tx, alice_vote_params) = th.dao_vote(
            &Holder::Alice,
            &dao_keypair,
            true,
            &dao,
            &propose_info,
            &propose_params.proposal_bulla,
        )?;

        info!("[Bob] Building vote tx (no)");
        let (bob_vote_tx, bob_vote_params) = th.dao_vote(
            &Holder::Bob,
            &dao_keypair,
            false,
            &dao,
            &propose_info,
            &propose_params.proposal_bulla,
        )?;

        info!("[Charlie] Building vote tx (yes)");
        let (charlie_vote_tx, charlie_vote_params) = th.dao_vote(
            &Holder::Charlie,
            &dao_keypair,
            true,
            &dao,
            &propose_info,
            &propose_params.proposal_bulla,
        )?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing Alice vote tx");
            th.execute_dao_vote_tx(holder, &alice_vote_tx, &alice_vote_params, current_slot)
                .await?;
            info!("[{holder:?}] Executing Bob vote tx");
            th.execute_dao_vote_tx(holder, &bob_vote_tx, &bob_vote_params, current_slot).await?;
            info!("[{holder:?}] Executing Charlie vote tx");
            th.execute_dao_vote_tx(holder, &charlie_vote_tx, &charlie_vote_params, current_slot)
                .await?;
        }

        // Gather and decrypt all vote notes
        let vote_note_1: DaoVoteNote = alice_vote_params.note.decrypt(&dao_keypair.secret).unwrap();
        let vote_note_2: DaoVoteNote = bob_vote_params.note.decrypt(&dao_keypair.secret).unwrap();
        let vote_note_3: DaoVoteNote =
            charlie_vote_params.note.decrypt(&dao_keypair.secret).unwrap();

        // Count the votes
        let mut total_yes_vote_value = 0;
        let mut total_all_vote_value = 0;
        let mut blind_total_vote = DaoBlindAggregateVote::default();
        let mut total_yes_vote_blind = pallas::Scalar::ZERO;
        let mut total_all_vote_blind = pallas::Scalar::ZERO;

        for (i, note) in [vote_note_1, vote_note_2, vote_note_3].iter().enumerate() {
            total_yes_vote_blind += note.yes_vote_blind;
            total_all_vote_blind += note.all_vote_blind;

            // Update private values
            // vote_option is either 0 or 1
            let yes_vote_value = note.vote_option as u64 * note.all_vote_value;
            total_yes_vote_value += yes_vote_value;
            total_all_vote_value += note.all_vote_value;

            // Update public values
            let yes_vote_commit = pedersen_commitment_u64(yes_vote_value, note.yes_vote_blind);
            let all_vote_commit = pedersen_commitment_u64(note.all_vote_value, note.all_vote_blind);
            let blind_vote = DaoBlindAggregateVote { yes_vote_commit, all_vote_commit };
            blind_total_vote.aggregate(blind_vote);

            // Just for the debug
            let vote_result = match note.vote_option {
                true => "yes",
                false => "no",
            };

            info!("Voter {} voted {} with {} tokens", i, vote_result, note.all_vote_value);
        }

        info!("Outcome = {} / {}", total_yes_vote_value, total_all_vote_value);

        assert!(
            blind_total_vote.all_vote_commit ==
                pedersen_commitment_u64(total_all_vote_value, total_all_vote_blind)
        );

        assert!(
            blind_total_vote.yes_vote_commit ==
                pedersen_commitment_u64(total_yes_vote_value, total_yes_vote_blind)
        );

        // ================
        // Dao::Exec
        // Execute the vote
        // ================
        info!("Stage 6. Execute the vote");

        info!("[Dao] Building Dao::Exec tx");
        let (exec_tx, xfer_params, exec_params) = th.dao_exec(
            &dao,
            &dao_mint_params.dao_bulla,
            &propose_info,
            total_yes_vote_value,
            total_all_vote_value,
            total_yes_vote_blind,
            total_all_vote_blind,
        )?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing Dao::Exec tx");
            th.execute_dao_exec_tx(holder, &exec_tx, &xfer_params, &exec_params, current_slot)
                .await?;
        }

        th.assert_trees(&HOLDERS);

        // Gather the coins
        th.gather_owncoin(&Holder::Rachel, &xfer_params.outputs[0], None)?;
        th.gather_owncoin(&Holder::Dao, &xfer_params.outputs[1], None)?;

        let rachel_wallet = th.holders.get(&Holder::Rachel).unwrap();
        assert!(rachel_wallet.unspent_money_coins[0].note.value == PROPOSAL_AMOUNT);
        assert!(rachel_wallet.unspent_money_coins[0].note.token_id == drk_token_id);

        // FIXME: The harness doesn't register that we spent the first coin on the proposal.
        let dao_wallet = th.holders.get(&Holder::Dao).unwrap();
        assert!(dao_wallet.unspent_money_coins[1].note.value == DRK_TOKEN_SUPPLY - PROPOSAL_AMOUNT);
        assert!(dao_wallet.unspent_money_coins[1].note.token_id == drk_token_id);

        // Stats
        th.statistics();

        // Thanks for reading
        Ok(())
    })
}
