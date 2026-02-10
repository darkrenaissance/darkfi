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

//! DAO integration test.
//!
//! Tests the full DAO lifecycle:
//!   1. Airdrop treasury tokens (DRK) to the DAO wallet
//!   2. Create the DAO on-chain (Dao::Mint)
//!   3. Distribute governance tokens to three members
//!   4. Transfer proposal: propose, vote (2 yes / 1 no), exec
//!   5. Transfer proposal with early execution
//!   6. Generic proposal: propose, vote, exec
//!   7. Generic proposal with early execution
//!
//! Holders:
//!   * Alice, Bob, Charlie — DAO members (governance token holders)
//!   * Dao — the DAO's treasury wallet
//!   * Rachel — recipient of transfer proposals

use darkfi::{util::pcg::Pcg32, Result};
use darkfi_contract_test_harness::{init_logger, Holder, TestHarness};
use darkfi_dao_contract::{
    blockwindow,
    model::{Dao, DaoBlindAggregateVote, DaoVoteParams},
    DaoFunction,
};
use darkfi_money_contract::model::{CoinAttributes, DARK_TOKEN_ID};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::*,
        pedersen_commitment_u64,
        util::{fp_mod_fv, fp_to_u64},
        BaseBlind, Blind, FuncId, FuncRef, Keypair, ScalarBlind, SecretKey, DAO_CONTRACT_ID,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::info;

// DAO governance token distribution
const ALICE_GOV_SUPPLY: u64 = 100_000_000;
const BOB_GOV_SUPPLY: u64 = 100_000_000;
const CHARLIE_GOV_SUPPLY: u64 = 100_000_000;

// DRK treasury supply
const DRK_TOKEN_SUPPLY: [u64; 1] = [1_000_000_000];

// DAO parameters
const PROPOSER_LIMIT: u64 = 100_000_000;
const QUORUM: u64 = 200_000_000;
const EARLY_EXEC_QUORUM: u64 = 200_000_000;
const APPROVAL_RATIO_BASE: u64 = 2;
const APPROVAL_RATIO_QUOT: u64 = 1;
const PROPOSAL_DURATION_BLOCKWINDOW: u64 = 1;

// Transfer proposal amount
const TRANSFER_PROPOSAL_AMOUNT: u64 = 250_000_000;

/// Collects the DAO keypairs so they can be passed around without
/// repeating 6 secret key arguments everywhere.
struct DaoKeys {
    notes: Keypair,
    proposer: Keypair,
    proposals: Keypair,
    votes: Keypair,
    exec: Keypair,
    early_exec: Keypair,
}

#[test]
fn integration_test() -> Result<()> {
    smol::block_on(async {
        init_logger();

        use Holder::{Alice, Bob, Charlie, Dao, Rachel};

        let mut th = TestHarness::new(&[Alice, Bob, Charlie, Dao, Rachel], false).await?;
        let mut height: u32 = 0;

        // Derive DAO governance token ID from Alice's mint authority
        let gov_token_blind = BaseBlind::random(&mut OsRng);
        let gov_token_id = th.derive_token_id(&Alice, gov_token_blind);

        // DAO keypairs
        let mut rng = Pcg32::new(42);
        let dao_keys = DaoKeys {
            notes: th.wallet(&Dao).keypair,
            proposer: Keypair::random(&mut rng),
            proposals: Keypair::random(&mut rng),
            votes: Keypair::random(&mut rng),
            exec: Keypair::random(&mut rng),
            early_exec: Keypair::random(&mut rng),
        };

        let dao = darkfi_dao_contract::model::Dao {
            proposer_limit: PROPOSER_LIMIT,
            quorum: QUORUM,
            early_exec_quorum: EARLY_EXEC_QUORUM,
            approval_ratio_base: APPROVAL_RATIO_BASE,
            approval_ratio_quot: APPROVAL_RATIO_QUOT,
            gov_token_id,
            notes_public_key: dao_keys.notes.public,
            proposer_public_key: dao_keys.proposer.public,
            proposals_public_key: dao_keys.proposals.public,
            votes_public_key: dao_keys.votes.public,
            exec_public_key: dao_keys.exec.public,
            early_exec_public_key: dao_keys.early_exec.public,
            bulla_blind: Blind::random(&mut OsRng),
        };

        // ===========================================
        // 1. Airdrop DRK treasury tokens to the DAO
        // ===========================================
        info!("Airdropping DRK treasury tokens to DAO");
        let spend_hook =
            FuncRef { contract_id: *DAO_CONTRACT_ID, func_code: DaoFunction::Exec as u8 }
                .to_func_id();

        let (genesis_tx, genesis_params) = th
            .genesis_mint(&Dao, &DRK_TOKEN_SUPPLY, Some(spend_hook), Some(dao.to_bulla().inner()))
            .await?;
        th.genesis_mint_to_all_with(genesis_tx, &genesis_params, height).await?;

        assert_eq!(th.coins(&Dao).len(), 1);
        assert_eq!(th.coins(&Dao)[0].note.token_id, *DARK_TOKEN_ID);
        assert_eq!(th.coins(&Dao)[0].note.value, DRK_TOKEN_SUPPLY[0]);
        height += 1;

        // ===========================
        // 2. Create the DAO on-chain
        // ===========================
        info!("Creating DAO bulla on-chain");
        th.dao_mint_to_all(
            &Alice,
            &dao,
            &dao_keys.notes.secret,
            &dao_keys.proposer.secret,
            &dao_keys.proposals.secret,
            &dao_keys.votes.secret,
            &dao_keys.exec.secret,
            &dao_keys.early_exec.secret,
            height,
        )
        .await?;
        height += 1;

        // =============================================
        // 3. Mint governance tokens to the three members
        // =============================================
        info!("Minting governance tokens to Alice, Bob, Charlie");
        th.token_mint_with_blind_to_all(ALICE_GOV_SUPPLY, &Alice, &Alice, gov_token_blind, height)
            .await?;

        assert_eq!(th.coins(&Alice).len(), 1);
        assert_eq!(th.balance(&Alice, gov_token_id), ALICE_GOV_SUPPLY);

        th.token_mint_with_blind_to_all(BOB_GOV_SUPPLY, &Alice, &Bob, gov_token_blind, height)
            .await?;

        assert_eq!(th.coins(&Bob).len(), 1);
        assert_eq!(th.balance(&Bob, gov_token_id), BOB_GOV_SUPPLY);

        th.token_mint_with_blind_to_all(
            CHARLIE_GOV_SUPPLY,
            &Alice,
            &Charlie,
            gov_token_blind,
            height,
        )
        .await?;

        assert_eq!(th.coins(&Charlie).len(), 1);
        assert_eq!(th.balance(&Charlie, gov_token_id), CHARLIE_GOV_SUPPLY);
        height += 1;

        let user_data = pallas::Base::ZERO;

        // ============================================
        // 4. Transfer proposal (normal execution)
        // ============================================
        info!("DAO transfer proposal — normal execution");
        execute_transfer_proposal(
            &mut th,
            &dao,
            &dao_keys,
            &None,
            user_data,
            &mut height,
            0,
            TRANSFER_PROPOSAL_AMOUNT,
            TRANSFER_PROPOSAL_AMOUNT,
        )
        .await?;

        // ============================================
        // 5. Transfer proposal (early execution)
        // ============================================
        info!("DAO transfer proposal — early execution");
        execute_transfer_proposal(
            &mut th,
            &dao,
            &dao_keys,
            &Some(dao_keys.early_exec.secret),
            user_data,
            &mut height,
            1,
            TRANSFER_PROPOSAL_AMOUNT,
            TRANSFER_PROPOSAL_AMOUNT * 2,
        )
        .await?;

        // ============================================
        // 6. Generic proposal (normal execution)
        // ============================================
        info!("DAO generic proposal — normal execution");
        execute_generic_proposal(&mut th, &dao, &dao_keys, &None, user_data, &mut height).await?;

        // Mint more governance tokens to refresh the merkle tree snapshot
        // before the next proposal round.
        info!("Minting additional governance tokens (snapshot refresh)");
        th.token_mint_with_blind_to_all(ALICE_GOV_SUPPLY, &Alice, &Alice, gov_token_blind, height)
            .await?;
        height += 1;

        // ============================================
        // 7. Generic proposal (early execution)
        // ============================================
        info!("DAO generic proposal — early execution");
        execute_generic_proposal(
            &mut th,
            &dao,
            &dao_keys,
            &Some(dao_keys.early_exec.secret),
            user_data,
            &mut height,
        )
        .await?;

        // Thanks for reading
        Ok(())
    })
}

// =========================================================================
// Proposal execution helpers
// =========================================================================

/// Execute a transfer proposal: propose → vote → (wait if not early) → exec.
///
/// Asserts that Rachel received the transfer and the DAO treasury decreased
/// by `dao_treasury_decrease`.
#[allow(clippy::too_many_arguments)]
async fn execute_transfer_proposal(
    th: &mut TestHarness,
    dao: &Dao,
    keys: &DaoKeys,
    early_exec_secret: &Option<SecretKey>,
    user_data: pallas::Base,
    height: &mut u32,
    rachel_coin_index: usize,
    transfer_amount: u64,
    dao_treasury_decrease: u64,
) -> Result<()> {
    let proposal_coinattrs = vec![CoinAttributes {
        public_key: th.wallet(&Holder::Rachel).keypair.public,
        value: transfer_amount,
        token_id: *DARK_TOKEN_ID,
        spend_hook: FuncId::none(),
        user_data: pallas::Base::ZERO,
        blind: Blind::random(&mut OsRng),
    }];

    // Grab creation blockwindow for expiry calculation
    let block_target = th.wallet(&Holder::Dao).validator.read().await.consensus.module.target;
    let creation_blockwindow = blockwindow(*height, block_target);

    // Propose
    info!("Building transfer proposal");
    let proposal_info = th
        .dao_propose_transfer_to_all(
            &Holder::Alice,
            &proposal_coinattrs,
            user_data,
            dao,
            &keys.proposer.secret,
            *height,
            PROPOSAL_DURATION_BLOCKWINDOW,
        )
        .await?;
    *height += 1;

    // Vote: Alice=yes, Bob=no, Charlie=yes → 2/3 majority
    let (total_yes, total_all, yes_blind, all_blind) =
        run_vote_round(th, dao, keys, &proposal_info, *height).await?;

    // Wait for proposal expiry (unless early exec)
    if early_exec_secret.is_none() {
        wait_for_proposal_expiry(height, creation_blockwindow, block_target);
    }

    // Execute
    info!("Executing transfer proposal");
    th.dao_exec_transfer_to_all(
        &Holder::Alice,
        dao,
        &keys.exec.secret,
        early_exec_secret,
        &proposal_info,
        proposal_coinattrs,
        total_yes,
        total_all,
        yes_blind,
        all_blind,
        *height,
    )
    .await?;
    *height += 1;

    // Assert Rachel received the transfer
    assert_eq!(th.coins(&Holder::Rachel)[rachel_coin_index].note.value, transfer_amount);
    assert_eq!(th.coins(&Holder::Rachel)[rachel_coin_index].note.token_id, *DARK_TOKEN_ID);

    // Assert DAO treasury decreased correctly
    assert_eq!(th.coins(&Holder::Dao)[0].note.value, DRK_TOKEN_SUPPLY[0] - dao_treasury_decrease);
    assert_eq!(th.coins(&Holder::Dao)[0].note.token_id, *DARK_TOKEN_ID);

    Ok(())
}

/// Execute a generic proposal: propose → vote → (wait if not early) → exec.
async fn execute_generic_proposal(
    th: &mut TestHarness,
    dao: &Dao,
    keys: &DaoKeys,
    early_exec_secret: &Option<SecretKey>,
    user_data: pallas::Base,
    height: &mut u32,
) -> Result<()> {
    // Grab creation blockwindow for expiry calculation
    let block_target = th.wallet(&Holder::Dao).validator.read().await.consensus.module.target;
    let creation_blockwindow = blockwindow(*height, block_target);

    // Propose
    info!("Building generic proposal");
    let proposal_info = th
        .dao_propose_generic_to_all(
            &Holder::Alice,
            user_data,
            dao,
            &keys.proposer.secret,
            *height,
            PROPOSAL_DURATION_BLOCKWINDOW,
        )
        .await?;
    *height += 1;

    // Vote: Alice=yes, Bob=no, Charlie=yes → 2/3 majority
    let (total_yes, total_all, yes_blind, all_blind) =
        run_vote_round(th, dao, keys, &proposal_info, *height).await?;

    // Wait for proposal expiry (unless early exec)
    if early_exec_secret.is_none() {
        wait_for_proposal_expiry(height, creation_blockwindow, block_target);
    }

    // Execute
    info!("Executing generic proposal");
    th.dao_exec_generic_to_all(
        &Holder::Alice,
        dao,
        &keys.exec.secret,
        early_exec_secret,
        &proposal_info,
        total_yes,
        total_all,
        yes_blind,
        all_blind,
        *height,
    )
    .await?;
    *height += 1;

    Ok(())
}

// =========================================================================
// Shared vote/count/wait utilities
// =========================================================================

/// Run a complete vote round: Alice=yes, Bob=no, Charlie=yes.
/// Returns (total_yes_value, total_all_value, yes_blind, all_blind).
async fn run_vote_round(
    th: &mut TestHarness,
    dao: &Dao,
    keys: &DaoKeys,
    proposal: &darkfi_dao_contract::model::DaoProposal,
    height: u32,
) -> Result<(u64, u64, ScalarBlind, ScalarBlind)> {
    info!("Vote round: Alice=yes, Bob=no, Charlie=yes");

    let alice_vote = th.dao_vote_to_all(&Holder::Alice, true, dao, proposal, height).await?;
    let bob_vote = th.dao_vote_to_all(&Holder::Bob, false, dao, proposal, height).await?;
    let charlie_vote = th.dao_vote_to_all(&Holder::Charlie, true, dao, proposal, height).await?;

    // Decrypt vote notes and tally
    let note_a = alice_vote.note.decrypt_unsafe(&keys.votes.secret).unwrap();
    let note_b = bob_vote.note.decrypt_unsafe(&keys.votes.secret).unwrap();
    let note_c = charlie_vote.note.decrypt_unsafe(&keys.votes.secret).unwrap();

    Ok(count_votes(&[(note_a, alice_vote), (note_b, bob_vote), (note_c, charlie_vote)]))
}

/// Advance `height` past the proposal expiry blockwindow.
fn wait_for_proposal_expiry(height: &mut u32, creation_blockwindow: u64, block_target: u32) {
    let mut current_bw = creation_blockwindow;
    while current_bw <= creation_blockwindow + PROPOSAL_DURATION_BLOCKWINDOW {
        *height += 1;
        current_bw = blockwindow(*height, block_target);
    }
}

/// Tally votes from decrypted vote notes.
///
/// Verifies Pedersen commitment consistency between private tallies
/// and public aggregate commitments.
fn count_votes(
    votes: &[([pallas::Base; 4], DaoVoteParams)],
) -> (u64, u64, ScalarBlind, ScalarBlind) {
    let mut total_yes_vote_value = 0;
    let mut total_all_vote_value = 0;
    let mut blind_total_vote = DaoBlindAggregateVote::default();
    let mut total_yes_vote_blind = Blind::ZERO;
    let mut total_all_vote_blind = Blind::ZERO;

    for (i, (note, params)) in votes.iter().enumerate() {
        // Note layout: [vote_option, yes_vote_blind, all_vote_value, all_vote_blind]
        let vote_option = fp_to_u64(note[0]).unwrap();
        let yes_vote_blind = Blind(fp_mod_fv(note[1]));
        let all_vote_value = fp_to_u64(note[2]).unwrap();
        let all_vote_blind = Blind(fp_mod_fv(note[3]));
        assert!(vote_option == 0 || vote_option == 1);

        total_yes_vote_blind += yes_vote_blind;
        total_all_vote_blind += all_vote_blind;

        let yes_vote_value = vote_option * all_vote_value;
        total_yes_vote_value += yes_vote_value;
        total_all_vote_value += all_vote_value;

        let yes_vote_commit = params.yes_vote_commit;
        let all_vote_commit = params.inputs.iter().map(|i| i.vote_commit).sum();
        let blind_vote = DaoBlindAggregateVote { yes_vote_commit, all_vote_commit };
        blind_total_vote.aggregate(blind_vote);

        let label = if vote_option != 0 { "yes" } else { "no" };
        info!("Voter {i} voted {label} with {all_vote_value} tokens");
    }

    info!("Vote outcome = {total_yes_vote_value} / {total_all_vote_value}");

    assert_eq!(
        blind_total_vote.all_vote_commit,
        pedersen_commitment_u64(total_all_vote_value, total_all_vote_blind)
    );
    assert_eq!(
        blind_total_vote.yes_vote_commit,
        pedersen_commitment_u64(total_yes_vote_value, total_yes_vote_blind)
    );

    (total_yes_vote_value, total_all_vote_value, total_yes_vote_blind, total_all_vote_blind)
}
