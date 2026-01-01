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

use darkfi::{util::pcg::Pcg32, Result};
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use darkfi_dao_contract::{
    blockwindow,
    model::{Dao, DaoBlindAggregateVote, DaoVoteParams},
    DaoFunction,
};
use darkfi_money_contract::{
    model::{CoinAttributes, TokenAttributes, DARK_TOKEN_ID},
    MoneyFunction,
};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::*,
        pedersen_commitment_u64, poseidon_hash,
        util::{fp_mod_fv, fp_to_u64},
        BaseBlind, Blind, FuncId, FuncRef, Keypair, ScalarBlind, SecretKey, DAO_CONTRACT_ID,
        MONEY_CONTRACT_ID,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::info;

// Integration test configuration
// Holders this test will use:
// * Alice, Bob, and Charlie are members of the DAO.
// * Dao is the DAO wallet
// * Rachel is the transfer proposal recipient.
const HOLDERS: [Holder; 5] =
    [Holder::Alice, Holder::Bob, Holder::Charlie, Holder::Dao, Holder::Rachel];
// DAO gov tokens distribution
const ALICE_GOV_SUPPLY: u64 = 100_000_000;
const BOB_GOV_SUPPLY: u64 = 100_000_000;
const CHARLIE_GOV_SUPPLY: u64 = 100_000_000;
// DRK token, the treasury token, supply
const DRK_TOKEN_SUPPLY: [u64; 1] = [1_000_000_000];
// DAO parameters configuration
const PROPOSER_LIMIT: u64 = 100_000_000;
const QUORUM: u64 = 200_000_000;
const EARLY_EXEC_QUORUM: u64 = 200_000_000;
const APPROVAL_RATIO_BASE: u64 = 2;
const APPROVAL_RATIO_QUOT: u64 = 1;
const PROPOSAL_DURATION_BLOCKWINDOW: u64 = 1;
// The tokens we want to send via the transfer proposal
const TRANSFER_PROPOSAL_AMOUNT: u64 = 250_000_000;

#[test]
fn integration_test() -> Result<()> {
    smol::block_on(async {
        init_logger();

        // Initialize harness
        let mut th = TestHarness::new(&HOLDERS, false).await?;

        // We'll use the ALICE token as the DAO governance token
        let wallet = th.holders.get_mut(&Holder::Alice).unwrap();
        //wallet.bench_wasm = true;
        let mint_authority = wallet.token_mint_authority;
        let gov_token_blind = BaseBlind::random(&mut OsRng);

        let auth_func_id = FuncRef {
            contract_id: *MONEY_CONTRACT_ID,
            func_code: MoneyFunction::AuthTokenMintV1 as u8,
        }
        .to_func_id();
        let token_attrs = TokenAttributes {
            auth_parent: auth_func_id,
            user_data: poseidon_hash([mint_authority.public.x(), mint_authority.public.y()]),
            blind: gov_token_blind,
        };
        let gov_token_id = token_attrs.to_token_id();

        // Block height to verify against
        let mut current_block_height = 0;

        // DAO parameters
        let dao_notes_keypair = th.holders.get(&Holder::Dao).unwrap().keypair;
        let mut rng = Pcg32::new(42);
        let dao_proposer_keypair = Keypair::random(&mut rng);
        let dao_proposals_keypair = Keypair::random(&mut rng);
        let dao_votes_keypair = Keypair::random(&mut rng);
        let dao_exec_keypair = Keypair::random(&mut rng);
        let dao_early_exec_keypair = Keypair::random(&mut rng);
        let dao = Dao {
            proposer_limit: PROPOSER_LIMIT,
            quorum: QUORUM,
            early_exec_quorum: EARLY_EXEC_QUORUM,
            approval_ratio_base: APPROVAL_RATIO_BASE,
            approval_ratio_quot: APPROVAL_RATIO_QUOT,
            gov_token_id,
            notes_public_key: dao_notes_keypair.public,
            proposer_public_key: dao_proposer_keypair.public,
            proposals_public_key: dao_proposals_keypair.public,
            votes_public_key: dao_votes_keypair.public,
            exec_public_key: dao_exec_keypair.public,
            early_exec_public_key: dao_early_exec_keypair.public,
            bulla_blind: Blind::random(&mut OsRng),
        };

        // =======================================
        // Airdrop some treasury tokens to the DAO
        // =======================================
        info!("[Dao] Building DAO airdrop tx");

        let spend_hook =
            FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }
                .to_func_id();

        let (genesis_mint_tx, genesis_mint_params) = th
            .genesis_mint(
                &Holder::Dao,
                &DRK_TOKEN_SUPPLY,
                Some(spend_hook),
                Some(dao.to_bulla().inner()),
            )
            .await?;

        for holder in &HOLDERS {
            th.execute_genesis_mint_tx(
                holder,
                genesis_mint_tx.clone(),
                &genesis_mint_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        let _dao_tokens = &th.holders.get(&Holder::Dao).unwrap().unspent_money_coins;
        assert!(_dao_tokens.len() == 1);
        assert!(_dao_tokens[0].note.token_id == *DARK_TOKEN_ID);
        assert!(_dao_tokens[0].note.value == DRK_TOKEN_SUPPLY[0]);

        current_block_height += 1;

        // ====================
        // Dao::Mint
        // Create the DAO bulla
        // ====================
        info!("[Dao] Building DAO mint tx");
        let (dao_mint_tx, dao_mint_params, fee_params) = th
            .dao_mint(
                &Holder::Alice,
                &dao,
                &dao_notes_keypair.secret,
                &dao_proposer_keypair.secret,
                &dao_proposals_keypair.secret,
                &dao_votes_keypair.secret,
                &dao_exec_keypair.secret,
                &dao_early_exec_keypair.secret,
                current_block_height,
            )
            .await?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing DAO Mint tx");
            th.execute_dao_mint_tx(
                holder,
                dao_mint_tx.clone(),
                &dao_mint_params,
                &fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        current_block_height += 1;

        // ======================================
        // Mint the governance token to 3 holders
        // ======================================
        info!("[Dao] Building governance token mint tx for Alice");
        let (a_token_mint_tx, a_token_mint_params, a_auth_token_mint_params, a_fee_params) = th
            .token_mint(
                ALICE_GOV_SUPPLY,
                &Holder::Alice,
                &Holder::Alice,
                gov_token_blind,
                None,
                None,
                current_block_height,
            )
            .await?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing governance token mint tx for Alice");
            th.execute_token_mint_tx(
                holder,
                a_token_mint_tx.clone(),
                &a_token_mint_params,
                &a_auth_token_mint_params,
                &a_fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        let _alice_tokens = &th.holders.get(&Holder::Alice).unwrap().unspent_money_coins;
        assert!(_alice_tokens.len() == 1);
        assert!(_alice_tokens[0].note.token_id == gov_token_id);
        assert!(_alice_tokens[0].note.value == ALICE_GOV_SUPPLY);

        info!("[Dao] Building governance token mint tx for Bob");
        let (b_token_mint_tx, b_token_mint_params, b_auth_token_mint_params, b_fee_params) = th
            .token_mint(
                BOB_GOV_SUPPLY,
                &Holder::Alice,
                &Holder::Bob,
                gov_token_blind,
                None,
                None,
                current_block_height,
            )
            .await?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing governance token mint tx for Bob");
            th.execute_token_mint_tx(
                holder,
                b_token_mint_tx.clone(),
                &b_token_mint_params,
                &b_auth_token_mint_params,
                &b_fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        let _bob_tokens = &th.holders.get(&Holder::Bob).unwrap().unspent_money_coins;
        assert!(_bob_tokens.len() == 1);
        assert!(_bob_tokens[0].note.token_id == gov_token_id);
        assert!(_bob_tokens[0].note.value == BOB_GOV_SUPPLY);

        info!("[Dao] Building governance token mint tx for Charlie");
        let (c_token_mint_tx, c_token_mint_params, c_auth_token_mint_params, c_fee_params) = th
            .token_mint(
                CHARLIE_GOV_SUPPLY,
                &Holder::Alice,
                &Holder::Charlie,
                gov_token_blind,
                None,
                None,
                current_block_height,
            )
            .await?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing governance token mint tx for Charlie");
            th.execute_token_mint_tx(
                holder,
                c_token_mint_tx.clone(),
                &c_token_mint_params,
                &c_auth_token_mint_params,
                &c_fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        let _charlie_tokens = &th.holders.get(&Holder::Charlie).unwrap().unspent_money_coins;
        assert!(_charlie_tokens.len() == 1);
        assert!(_charlie_tokens[0].note.token_id == gov_token_id);
        assert!(_charlie_tokens[0].note.value == CHARLIE_GOV_SUPPLY);

        current_block_height += 1;

        // We can add whatever we want in here, even arbitrary text
        // It's up to the auth module to decide what to do with it.
        let user_data = pallas::Base::ZERO;

        // ============================
        // Execute proposals test cases
        // ============================
        info!("[Dao] DAO transfer proposal tx test case");
        execute_transfer_proposal(
            &mut th,
            &dao,
            &dao_proposer_keypair.secret,
            &dao_votes_keypair.secret,
            &dao_exec_keypair.secret,
            &None,
            user_data,
            &mut current_block_height,
            0,
            TRANSFER_PROPOSAL_AMOUNT,
            TRANSFER_PROPOSAL_AMOUNT,
        )
        .await?;

        info!("[Dao] DAO early execution transfer proposal tx test case");
        execute_transfer_proposal(
            &mut th,
            &dao,
            &dao_proposer_keypair.secret,
            &dao_votes_keypair.secret,
            &dao_exec_keypair.secret,
            &Some(dao_early_exec_keypair.secret),
            user_data,
            &mut current_block_height,
            1,
            TRANSFER_PROPOSAL_AMOUNT,
            TRANSFER_PROPOSAL_AMOUNT * 2,
        )
        .await?;

        info!("[Dao] DAO generic proposal tx test case");
        execute_generic_proposal(
            &mut th,
            &dao,
            &dao_proposer_keypair.secret,
            &dao_votes_keypair.secret,
            &dao_exec_keypair.secret,
            &None,
            user_data,
            &mut current_block_height,
        )
        .await?;

        // Now we will execute a random money transaction,
        // to update our merkle tree so our snapshot is fresh.
        info!("[Dao] Building governance token mint tx for Alice");
        let (a_token_mint_tx, a_token_mint_params, a_auth_token_mint_params, a_fee_params) = th
            .token_mint(
                ALICE_GOV_SUPPLY,
                &Holder::Alice,
                &Holder::Alice,
                gov_token_blind,
                None,
                None,
                current_block_height,
            )
            .await?;

        for holder in &HOLDERS {
            info!("[{holder:?}] Executing governance token mint tx for Alice");
            th.execute_token_mint_tx(
                holder,
                a_token_mint_tx.clone(),
                &a_token_mint_params,
                &a_auth_token_mint_params,
                &a_fee_params,
                current_block_height,
                true,
            )
            .await?;
        }

        th.assert_trees(&HOLDERS);

        current_block_height += 1;

        // Now we can continue our test cases
        info!("[Dao] DAO early execution generic proposal tx test case");
        execute_generic_proposal(
            &mut th,
            &dao,
            &dao_proposer_keypair.secret,
            &dao_votes_keypair.secret,
            &dao_exec_keypair.secret,
            &Some(dao_early_exec_keypair.secret),
            user_data,
            &mut current_block_height,
        )
        .await?;

        // Thanks for reading
        Ok(())
    })
}

/// Test case:
/// Generate a transfer proposal and execute it after voting passes.
#[allow(clippy::too_many_arguments)]
async fn execute_transfer_proposal(
    th: &mut TestHarness,
    dao: &Dao,
    dao_proposer_secret_key: &SecretKey,
    dao_votes_secret_key: &SecretKey,
    dao_exec_secret_key: &SecretKey,
    dao_early_exec_secret_key: &Option<SecretKey>,
    user_data: pallas::Base,
    current_block_height: &mut u32,
    transfer_token_index: usize,
    transfer_amount: u64,
    dao_treasury_decrease: u64,
) -> Result<()> {
    // ================
    // Dao::Propose
    // Propose the vote
    // ================
    info!("[Dao] Building DAO transfer proposal tx");

    // These coins are passed around to all DAO members who verify its validity
    // They also check hashing them equals the proposal_commit
    let proposal_coinattrs = vec![CoinAttributes {
        public_key: th.holders.get(&Holder::Rachel).unwrap().keypair.public,
        value: transfer_amount,
        token_id: *DARK_TOKEN_ID,
        spend_hook: FuncId::none(),
        user_data: pallas::Base::ZERO,
        blind: Blind::random(&mut OsRng),
    }];

    // Grab creation blockwindow
    let block_target =
        th.holders.get_mut(&Holder::Dao).unwrap().validator.consensus.module.read().await.target;
    let creation_blockwindow = blockwindow(*current_block_height, block_target);

    let (tx, params, fee_params, proposal_info) = th
        .dao_propose_transfer(
            &Holder::Alice,
            &proposal_coinattrs,
            user_data,
            dao,
            dao_proposer_secret_key,
            *current_block_height,
            PROPOSAL_DURATION_BLOCKWINDOW,
        )
        .await?;

    for holder in &HOLDERS {
        info!("[{holder:?}] Executing DAO transfer proposal tx");
        th.execute_dao_propose_tx(
            holder,
            tx.clone(),
            &params,
            &fee_params,
            *current_block_height,
            true,
        )
        .await?;
    }
    th.assert_trees(&HOLDERS);
    *current_block_height += 1;

    // =====================================
    // Dao::Vote
    // Proposal is accepted. Start the vote.
    // =====================================
    info!("[Alice] Building transfer vote tx (yes)");
    let (alice_vote_tx, alice_vote_params, alice_vote_fee_params) =
        th.dao_vote(&Holder::Alice, true, dao, &proposal_info, *current_block_height).await?;

    info!("[Bob] Building transfer vote tx (no)");
    let (bob_vote_tx, bob_vote_params, bob_vote_fee_params) =
        th.dao_vote(&Holder::Bob, false, dao, &proposal_info, *current_block_height).await?;

    info!("[Charlie] Building transfer vote tx (yes)");
    let (charlie_vote_tx, charlie_vote_params, charlie_vote_fee_params) =
        th.dao_vote(&Holder::Charlie, true, dao, &proposal_info, *current_block_height).await?;

    for holder in &HOLDERS {
        info!("[{holder:?}] Executing Alice transfer vote tx");
        th.execute_dao_vote_tx(
            holder,
            alice_vote_tx.clone(),
            &alice_vote_fee_params,
            *current_block_height,
            true,
        )
        .await?;

        info!("[{holder:?}] Executing Bob transfer vote tx");
        th.execute_dao_vote_tx(
            holder,
            bob_vote_tx.clone(),
            &bob_vote_fee_params,
            *current_block_height,
            true,
        )
        .await?;

        info!("[{holder:?}] Executing Charlie transfer vote tx");
        th.execute_dao_vote_tx(
            holder,
            charlie_vote_tx.clone(),
            &charlie_vote_fee_params,
            *current_block_height,
            true,
        )
        .await?;
    }
    th.assert_trees(&HOLDERS);

    // Gather and decrypt all generic vote notes
    let vote_note_1 = alice_vote_params.note.decrypt_unsafe(dao_votes_secret_key).unwrap();
    let vote_note_2 = bob_vote_params.note.decrypt_unsafe(dao_votes_secret_key).unwrap();
    let vote_note_3 = charlie_vote_params.note.decrypt_unsafe(dao_votes_secret_key).unwrap();

    // Count the votes
    let (total_yes_vote_value, total_all_vote_value, total_yes_vote_blind, total_all_vote_blind) =
        count_votes(&[
            (vote_note_1, alice_vote_params),
            (vote_note_2, bob_vote_params),
            (vote_note_3, charlie_vote_params),
        ]);

    // Wait until proposal has expired
    if dao_early_exec_secret_key.is_none() {
        let mut current_blockwindow = creation_blockwindow;
        while current_blockwindow <= creation_blockwindow + PROPOSAL_DURATION_BLOCKWINDOW {
            *current_block_height += 1;
            current_blockwindow = blockwindow(*current_block_height, block_target);
        }
    }

    // ================
    // Dao::Exec
    // Execute the vote
    // ================
    info!("[Dao] Building transfer Dao::Exec tx");
    let (exec_tx, xfer_params, exec_fee_params) = th
        .dao_exec_transfer(
            &Holder::Alice,
            dao,
            dao_exec_secret_key,
            dao_early_exec_secret_key,
            &proposal_info,
            proposal_coinattrs,
            total_yes_vote_value,
            total_all_vote_value,
            total_yes_vote_blind,
            total_all_vote_blind,
            *current_block_height,
        )
        .await?;

    for holder in &HOLDERS {
        info!("[{holder:?}] Executing transfer Dao::Exec tx");
        th.execute_dao_exec_tx(
            holder,
            exec_tx.clone(),
            Some(&xfer_params),
            &exec_fee_params,
            *current_block_height,
            true,
        )
        .await?;
    }
    th.assert_trees(&HOLDERS);
    *current_block_height += 1;

    let rachel_wallet = th.holders.get(&Holder::Rachel).unwrap();
    assert!(rachel_wallet.unspent_money_coins[transfer_token_index].note.value == transfer_amount);
    assert!(
        rachel_wallet.unspent_money_coins[transfer_token_index].note.token_id == *DARK_TOKEN_ID
    );

    let dao_wallet = th.holders.get(&Holder::Dao).unwrap();
    assert!(
        dao_wallet.unspent_money_coins[0].note.value == DRK_TOKEN_SUPPLY[0] - dao_treasury_decrease
    );
    assert!(dao_wallet.unspent_money_coins[0].note.token_id == *DARK_TOKEN_ID);

    Ok(())
}

/// Test case:
/// Generate a generic proposal and execute it after voting passes.
#[allow(clippy::too_many_arguments)]
async fn execute_generic_proposal(
    th: &mut TestHarness,
    dao: &Dao,
    dao_proposer_secret_key: &SecretKey,
    dao_votes_secret_key: &SecretKey,
    dao_exec_secret_key: &SecretKey,
    dao_early_exec_secret_key: &Option<SecretKey>,
    user_data: pallas::Base,
    current_block_height: &mut u32,
) -> Result<()> {
    // ================
    // Dao::Propose
    // Propose the vote
    // ================
    info!("[Dao] Building DAO generic proposal tx");

    // Grab creation blockwindow
    let block_target =
        th.holders.get_mut(&Holder::Dao).unwrap().validator.consensus.module.read().await.target;
    let creation_blockwindow = blockwindow(*current_block_height, block_target);

    let (tx, params, fee_params, proposal_info) = th
        .dao_propose_generic(
            &Holder::Alice,
            user_data,
            dao,
            dao_proposer_secret_key,
            *current_block_height,
            PROPOSAL_DURATION_BLOCKWINDOW,
        )
        .await?;

    for holder in &HOLDERS {
        info!("[{holder:?}] Executing DAO generic proposal tx");
        th.execute_dao_propose_tx(
            holder,
            tx.clone(),
            &params,
            &fee_params,
            *current_block_height,
            true,
        )
        .await?;
    }
    th.assert_trees(&HOLDERS);
    *current_block_height += 1;

    // =====================================
    // Dao::Vote
    // Proposal is accepted. Start the vote.
    // =====================================
    info!("[Alice] Building generic vote tx (yes)");
    let (alice_vote_tx, alice_vote_params, alice_vote_fee_params) =
        th.dao_vote(&Holder::Alice, true, dao, &proposal_info, *current_block_height).await?;

    info!("[Bob] Building generic vote tx (no)");
    let (bob_vote_tx, bob_vote_params, bob_vote_fee_params) =
        th.dao_vote(&Holder::Bob, false, dao, &proposal_info, *current_block_height).await?;

    info!("[Charlie] Building generic vote tx (no)");
    let (charlie_vote_tx, charlie_vote_params, charlie_vote_fee_params) =
        th.dao_vote(&Holder::Charlie, true, dao, &proposal_info, *current_block_height).await?;

    for holder in &HOLDERS {
        info!("[{holder:?}] Executing Alice generic vote tx");
        th.execute_dao_vote_tx(
            holder,
            alice_vote_tx.clone(),
            &alice_vote_fee_params,
            *current_block_height,
            true,
        )
        .await?;

        info!("[{holder:?}] Executing Bob generic vote tx");
        th.execute_dao_vote_tx(
            holder,
            bob_vote_tx.clone(),
            &bob_vote_fee_params,
            *current_block_height,
            true,
        )
        .await?;

        info!("[{holder:?}] Executing Charlie generic vote tx");
        th.execute_dao_vote_tx(
            holder,
            charlie_vote_tx.clone(),
            &charlie_vote_fee_params,
            *current_block_height,
            true,
        )
        .await?;
    }
    th.assert_trees(&HOLDERS);

    // Gather and decrypt all generic vote notes
    let vote_note_1 = alice_vote_params.note.decrypt_unsafe(dao_votes_secret_key).unwrap();
    let vote_note_2 = bob_vote_params.note.decrypt_unsafe(dao_votes_secret_key).unwrap();
    let vote_note_3 = charlie_vote_params.note.decrypt_unsafe(dao_votes_secret_key).unwrap();

    // Count the votes
    let (total_yes_vote_value, total_all_vote_value, total_yes_vote_blind, total_all_vote_blind) =
        count_votes(&[
            (vote_note_1, alice_vote_params),
            (vote_note_2, bob_vote_params),
            (vote_note_3, charlie_vote_params),
        ]);

    // Wait until proposal has expired
    if dao_early_exec_secret_key.is_none() {
        let mut current_blockwindow = creation_blockwindow;
        while current_blockwindow <= creation_blockwindow + PROPOSAL_DURATION_BLOCKWINDOW {
            *current_block_height += 1;
            current_blockwindow = blockwindow(*current_block_height, block_target);
        }
    }

    // ================
    // Dao::Exec
    // Execute the vote
    // ================
    info!("[Dao] Building generic Dao::Exec tx");
    let (exec_tx, exec_fee_params) = th
        .dao_exec_generic(
            &Holder::Alice,
            dao,
            dao_exec_secret_key,
            dao_early_exec_secret_key,
            &proposal_info,
            total_yes_vote_value,
            total_all_vote_value,
            total_yes_vote_blind,
            total_all_vote_blind,
            *current_block_height,
        )
        .await?;

    for holder in &HOLDERS {
        info!("[{holder:?}] Executing generic Dao::Exec tx");
        th.execute_dao_exec_tx(
            holder,
            exec_tx.clone(),
            None,
            &exec_fee_params,
            *current_block_height,
            true,
        )
        .await?;
    }
    th.assert_trees(&HOLDERS);
    *current_block_height += 1;

    Ok(())
}

/// Auxiliary function to count proposal votes.
fn count_votes(
    votes: &[([pallas::Base; 4], DaoVoteParams)],
) -> (u64, u64, ScalarBlind, ScalarBlind) {
    let mut total_yes_vote_value = 0;
    let mut total_all_vote_value = 0;
    let mut blind_total_vote = DaoBlindAggregateVote::default();
    let mut total_yes_vote_blind = Blind::ZERO;
    let mut total_all_vote_blind = Blind::ZERO;

    for (i, (note, params)) in votes.iter().enumerate() {
        // Note format: [
        //   vote_option,
        //   yes_vote_blind,
        //   all_vote_value_fp,
        //   all_vote_blind,
        // ]
        let vote_option = fp_to_u64(note[0]).unwrap();
        let yes_vote_blind = Blind(fp_mod_fv(note[1]));
        let all_vote_value = fp_to_u64(note[2]).unwrap();
        let all_vote_blind = Blind(fp_mod_fv(note[3]));
        assert!(vote_option == 0 || vote_option == 1);

        total_yes_vote_blind += yes_vote_blind;
        total_all_vote_blind += all_vote_blind;

        // Update private values
        // vote_option is either 0 or 1
        let yes_vote_value = vote_option * all_vote_value;
        total_yes_vote_value += yes_vote_value;
        total_all_vote_value += all_vote_value;

        // Update public values
        let yes_vote_commit = params.yes_vote_commit;
        let all_vote_commit = params.inputs.iter().map(|i| i.vote_commit).sum();
        let blind_vote = DaoBlindAggregateVote { yes_vote_commit, all_vote_commit };
        blind_total_vote.aggregate(blind_vote);

        // Just for the debug
        let vote_result = match vote_option != 0 {
            true => "yes",
            false => "no",
        };
        info!("Voter {i} voted {vote_result} with {all_vote_value} tokens in vote");
    }

    info!("Vote outcome = {total_yes_vote_value} / {total_all_vote_value}");

    assert!(
        blind_total_vote.all_vote_commit ==
            pedersen_commitment_u64(total_all_vote_value, total_all_vote_blind)
    );

    assert!(
        blind_total_vote.yes_vote_commit ==
            pedersen_commitment_u64(total_yes_vote_value, total_yes_vote_blind)
    );

    (total_yes_vote_value, total_all_vote_value, total_yes_vote_blind, total_all_vote_blind)
}
