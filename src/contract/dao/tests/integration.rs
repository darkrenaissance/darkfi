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
    client::{DaoInfo, DaoVoteNote},
    model::DaoBlindAggregateVote,
};
use darkfi_sdk::{
    crypto::{pasta_prelude::Field, pedersen_commitment_u64, DAO_CONTRACT_ID, DARK_TOKEN_ID},
    pasta::pallas,
};
use log::info;
use rand::rngs::OsRng;

#[async_std::test]
async fn integration_test() -> Result<()> {
    init_logger();

    // Holders this test will use:
    // * Faucet airdrops DRK
    // * Alice, Bob, and Charlie are members of the DAO.
    // * Rachel is the proposal recipient.
    // * Dao is the DAO wallet
    const HOLDERS: [Holder; 6] =
        [Holder::Faucet, Holder::Alice, Holder::Bob, Holder::Charlie, Holder::Rachel, Holder::Dao];

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
    let dao = DaoInfo {
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

    info!("[Faucet] Executing DAO Mint tx");
    th.execute_dao_mint_tx(Holder::Faucet, &dao_mint_tx, &dao_mint_params, current_slot).await?;

    info!("[Alice] Executing DAO Mint tx");
    th.execute_dao_mint_tx(Holder::Alice, &dao_mint_tx, &dao_mint_params, current_slot).await?;

    info!("[Bob] Executing DAO Mint tx");
    th.execute_dao_mint_tx(Holder::Bob, &dao_mint_tx, &dao_mint_params, current_slot).await?;

    info!("[Charlie] Executing DAO Mint tx");
    th.execute_dao_mint_tx(Holder::Charlie, &dao_mint_tx, &dao_mint_params, current_slot).await?;

    info!("[Rachel] Executing DAO Mint tx");
    th.execute_dao_mint_tx(Holder::Rachel, &dao_mint_tx, &dao_mint_params, current_slot).await?;

    info!("[Dao] Executing DAO Mint tx");
    th.execute_dao_mint_tx(Holder::Dao, &dao_mint_tx, &dao_mint_params, current_slot).await?;

    // TODO: assert_trees

    // =======================================
    // Airdrop some treasury tokens to the DAO
    // =======================================
    info!("Stage 2. Send Treasury token");

    info!("[Faucet] Building DAO airdrop tx");
    let (airdrop_tx, airdrop_params) = th.airdrop_native(
        DRK_TOKEN_SUPPLY,
        Holder::Dao,
        Some(DAO_CONTRACT_ID.inner()),           // spend_hook
        Some(dao_mint_params.dao_bulla.inner()), // user_data
        None,
        None,
    )?;

    info!("[Faucet] Executing DAO airdrop tx");
    th.execute_airdrop_native_tx(Holder::Faucet, &airdrop_tx, &airdrop_params, current_slot)
        .await?;

    info!("[Alice] Executing DAO airdrop tx");
    th.execute_airdrop_native_tx(Holder::Alice, &airdrop_tx, &airdrop_params, current_slot).await?;

    info!("[Bob] Executing DAO airdrop tx");
    th.execute_airdrop_native_tx(Holder::Bob, &airdrop_tx, &airdrop_params, current_slot).await?;

    info!("[Charlie] Executing DAO airdrop tx");
    th.execute_airdrop_native_tx(Holder::Charlie, &airdrop_tx, &airdrop_params, current_slot)
        .await?;

    info!("[Rachel] Executing DAO airdrop tx");
    th.execute_airdrop_native_tx(Holder::Rachel, &airdrop_tx, &airdrop_params, current_slot)
        .await?;

    info!("[Dao] Executing DAO airdrop tx");
    th.execute_airdrop_native_tx(Holder::Dao, &airdrop_tx, &airdrop_params, current_slot).await?;

    th.assert_trees(&HOLDERS);

    // Gather the DAO owncoin
    th.gather_owncoin(Holder::Dao, airdrop_params.outputs[0].clone(), None)?;

    // ======================================
    // Mint the governance token to 3 holders
    // ======================================
    info!("Stage 3. Minting governance token");

    info!("[Alice] Building governance token mint tx for Alice");
    let (a_token_mint_tx, a_token_mint_params) =
        th.token_mint(ALICE_GOV_SUPPLY, Holder::Alice, Holder::Alice, None, None)?;

    info!("[Faucet] Executing governance token mint tx for Alice");
    th.execute_token_mint_tx(Holder::Faucet, &a_token_mint_tx, &a_token_mint_params, current_slot)
        .await?;

    info!("[Alice] Executing governance token mint tx for Alice");
    th.execute_token_mint_tx(Holder::Alice, &a_token_mint_tx, &a_token_mint_params, current_slot)
        .await?;

    info!("[Bob] Executing governance token mint tx for Alice");
    th.execute_token_mint_tx(Holder::Bob, &a_token_mint_tx, &a_token_mint_params, current_slot)
        .await?;

    info!("[Charlie] Executing governance token mint tx for Alice");
    th.execute_token_mint_tx(Holder::Charlie, &a_token_mint_tx, &a_token_mint_params, current_slot)
        .await?;

    info!("[Rachel] Executing governance token mint tx for Alice");
    th.execute_token_mint_tx(Holder::Rachel, &a_token_mint_tx, &a_token_mint_params, current_slot)
        .await?;

    info!("[Dao] Executing governance token mint tx for Alice");
    th.execute_token_mint_tx(Holder::Dao, &a_token_mint_tx, &a_token_mint_params, current_slot)
        .await?;

    th.assert_trees(&HOLDERS);

    // Gather owncoin
    th.gather_owncoin(Holder::Alice, a_token_mint_params.output, None)?;

    info!("[Alice] Building governance token mint tx for Bob");
    let (b_token_mint_tx, b_token_mint_params) =
        th.token_mint(BOB_GOV_SUPPLY, Holder::Alice, Holder::Bob, None, None)?;

    info!("[Faucet] Executing governance token mint tx for Bob");
    th.execute_token_mint_tx(Holder::Faucet, &b_token_mint_tx, &b_token_mint_params, current_slot)
        .await?;

    info!("[Alice] Executing governance token mint tx for Bob");
    th.execute_token_mint_tx(Holder::Alice, &b_token_mint_tx, &b_token_mint_params, current_slot)
        .await?;

    info!("[Bob] Executing governance token mint tx for Bob");
    th.execute_token_mint_tx(Holder::Bob, &b_token_mint_tx, &b_token_mint_params, current_slot)
        .await?;

    info!("[Charlie] Executing governance token mint tx for Bob");
    th.execute_token_mint_tx(Holder::Charlie, &b_token_mint_tx, &b_token_mint_params, current_slot)
        .await?;

    info!("[Rachel] Executing governance token mint tx for Bob");
    th.execute_token_mint_tx(Holder::Rachel, &b_token_mint_tx, &b_token_mint_params, current_slot)
        .await?;

    info!("[Dao] Executing governance token mint tx for Bob");
    th.execute_token_mint_tx(Holder::Dao, &b_token_mint_tx, &b_token_mint_params, current_slot)
        .await?;

    th.assert_trees(&HOLDERS);

    // Gather owncoin
    th.gather_owncoin(Holder::Bob, b_token_mint_params.output, None)?;

    info!("[Alice] Building governance token mint tx for Charlie");
    let (c_token_mint_tx, c_token_mint_params) =
        th.token_mint(CHARLIE_GOV_SUPPLY, Holder::Alice, Holder::Charlie, None, None)?;

    info!("[Faucet] Executing governance token mint tx for Charlie");
    th.execute_token_mint_tx(Holder::Faucet, &c_token_mint_tx, &c_token_mint_params, current_slot)
        .await?;

    info!("[Alice] Executing governance token mint tx for Charlie");
    th.execute_token_mint_tx(Holder::Alice, &c_token_mint_tx, &c_token_mint_params, current_slot)
        .await?;

    info!("[Bob] Executing governance token mint tx for Charlie");
    th.execute_token_mint_tx(Holder::Bob, &c_token_mint_tx, &c_token_mint_params, current_slot)
        .await?;

    info!("[Charlie] Executing governance token mint tx for Charlie");
    th.execute_token_mint_tx(Holder::Charlie, &c_token_mint_tx, &c_token_mint_params, current_slot)
        .await?;

    info!("[Rachel] Executing governance token mint tx for Charlie");
    th.execute_token_mint_tx(Holder::Rachel, &c_token_mint_tx, &c_token_mint_params, current_slot)
        .await?;

    info!("[Dao] Executing governance token mint tx for Charlie");
    th.execute_token_mint_tx(Holder::Dao, &c_token_mint_tx, &c_token_mint_params, current_slot)
        .await?;

    th.assert_trees(&HOLDERS);

    // Gather owncoin
    th.gather_owncoin(Holder::Charlie, c_token_mint_params.output, None)?;

    // ================
    // Dao::Propose
    // Propose the vote
    // ================
    info!("Stage 4. Propose the vote");
    // TODO: look into proposal expiry once time for voting has finished
    // TODO: Is it possible for an invalid transfer() to be constructed on exec()?
    //       Need to look into this.
    info!("[Alice] Building DAO proposal tx");
    let (propose_tx, propose_params, propose_info) = th.dao_propose(
        Holder::Alice,
        Holder::Rachel,
        PROPOSAL_AMOUNT,
        drk_token_id,
        dao.clone(),
        dao_mint_params.dao_bulla,
    )?;

    info!("[Faucet] Executing DAO proposal tx");
    th.execute_dao_propose_tx(Holder::Faucet, &propose_tx, &propose_params, current_slot).await?;

    info!("[Alice] Executing DAO proposal tx");
    th.execute_dao_propose_tx(Holder::Alice, &propose_tx, &propose_params, current_slot).await?;

    info!("[Bob] Executing DAO proposal tx");
    th.execute_dao_propose_tx(Holder::Bob, &propose_tx, &propose_params, current_slot).await?;

    info!("[Charlie] Executing DAO proposal tx");
    th.execute_dao_propose_tx(Holder::Charlie, &propose_tx, &propose_params, current_slot).await?;

    info!("[Rachel] Executing DAO proposal tx");
    th.execute_dao_propose_tx(Holder::Rachel, &propose_tx, &propose_params, current_slot).await?;

    info!("[Dao] Executing DAO proposal tx");
    th.execute_dao_propose_tx(Holder::Dao, &propose_tx, &propose_params, current_slot).await?;

    th.assert_trees(&HOLDERS);

    // =====================================
    // Dao::Vote
    // Proposal is accepted. Start the vote.
    // =====================================
    info!("Stage 5. Start voting");

    info!("[Alice] Building vote tx (yes)");
    let (alice_vote_tx, alice_vote_params) = th.dao_vote(
        Holder::Alice,
        &dao_keypair,
        true,
        dao.clone(),
        propose_info.clone(),
        propose_params.proposal_bulla,
    )?;

    info!("[Bob] Building vote tx (no)");
    let (bob_vote_tx, bob_vote_params) = th.dao_vote(
        Holder::Bob,
        &dao_keypair,
        false,
        dao.clone(),
        propose_info.clone(),
        propose_params.proposal_bulla,
    )?;

    info!("[Charlie] Building vote tx (yes)");
    let (charlie_vote_tx, charlie_vote_params) = th.dao_vote(
        Holder::Charlie,
        &dao_keypair,
        true,
        dao.clone(),
        propose_info.clone(),
        propose_params.proposal_bulla,
    )?;

    info!("[Faucet] Executing Alice vote tx");
    th.execute_dao_vote_tx(Holder::Faucet, &alice_vote_tx, &alice_vote_params, current_slot)
        .await?;
    info!("[Faucet] Executing Bob vote tx");
    th.execute_dao_vote_tx(Holder::Faucet, &bob_vote_tx, &bob_vote_params, current_slot).await?;
    info!("[Faucet] Executing Charlie vote tx");
    th.execute_dao_vote_tx(Holder::Faucet, &charlie_vote_tx, &charlie_vote_params, current_slot)
        .await?;

    info!("[Alice] Executing Alice vote tx");
    th.execute_dao_vote_tx(Holder::Alice, &alice_vote_tx, &alice_vote_params, current_slot).await?;
    info!("[Alice] Executing Bob vote tx");
    th.execute_dao_vote_tx(Holder::Alice, &bob_vote_tx, &bob_vote_params, current_slot).await?;
    info!("[Alice] Executing Charlie vote tx");
    th.execute_dao_vote_tx(Holder::Alice, &charlie_vote_tx, &charlie_vote_params, current_slot)
        .await?;

    info!("[Bob] Executing Alice vote tx");
    th.execute_dao_vote_tx(Holder::Bob, &alice_vote_tx, &alice_vote_params, current_slot).await?;
    info!("[Bob] Executing Bob vote tx");
    th.execute_dao_vote_tx(Holder::Bob, &bob_vote_tx, &bob_vote_params, current_slot).await?;
    info!("[Bob] Executing Charlie vote tx");
    th.execute_dao_vote_tx(Holder::Bob, &charlie_vote_tx, &charlie_vote_params, current_slot)
        .await?;

    info!("[Charlie] Executing Alice vote tx");
    th.execute_dao_vote_tx(Holder::Charlie, &alice_vote_tx, &alice_vote_params, current_slot)
        .await?;
    info!("[Charlie] Executing Bob vote tx");
    th.execute_dao_vote_tx(Holder::Charlie, &bob_vote_tx, &bob_vote_params, current_slot).await?;
    info!("[Charlie] Executing Charlie vote tx");
    th.execute_dao_vote_tx(Holder::Charlie, &charlie_vote_tx, &charlie_vote_params, current_slot)
        .await?;

    info!("[Rachel] Executing Alice vote tx");
    th.execute_dao_vote_tx(Holder::Rachel, &alice_vote_tx, &alice_vote_params, current_slot)
        .await?;
    info!("[Rachel] Executing Bob vote tx");
    th.execute_dao_vote_tx(Holder::Rachel, &bob_vote_tx, &bob_vote_params, current_slot).await?;
    info!("[Rachel] Executing Charlie vote tx");
    th.execute_dao_vote_tx(Holder::Rachel, &charlie_vote_tx, &charlie_vote_params, current_slot)
        .await?;

    info!("[Dao] Executing Alice vote tx");
    th.execute_dao_vote_tx(Holder::Dao, &alice_vote_tx, &alice_vote_params, current_slot).await?;
    info!("[Dao] Executing Bob vote tx");
    th.execute_dao_vote_tx(Holder::Dao, &bob_vote_tx, &bob_vote_params, current_slot).await?;
    info!("[Dao] Executing Charlie vote tx");
    th.execute_dao_vote_tx(Holder::Dao, &charlie_vote_tx, &charlie_vote_params, current_slot)
        .await?;

    // Gather and decrypt all vote notes
    let vote_note_1: DaoVoteNote = alice_vote_params.note.decrypt(&dao_keypair.secret).unwrap();
    let vote_note_2: DaoVoteNote = bob_vote_params.note.decrypt(&dao_keypair.secret).unwrap();
    let vote_note_3: DaoVoteNote = charlie_vote_params.note.decrypt(&dao_keypair.secret).unwrap();

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
        dao,
        dao_mint_params.dao_bulla,
        propose_info,
        total_yes_vote_value,
        total_all_vote_value,
        total_yes_vote_blind,
        total_all_vote_blind,
    )?;

    info!("[Faucet] Executing Dao::Exec tx");
    th.execute_dao_exec_tx(Holder::Faucet, &exec_tx, &xfer_params, &exec_params, current_slot)
        .await?;

    info!("[Alice] Executing Dao::Exec tx");
    th.execute_dao_exec_tx(Holder::Alice, &exec_tx, &xfer_params, &exec_params, current_slot)
        .await?;

    info!("[Bob] Executing Dao::Exec tx");
    th.execute_dao_exec_tx(Holder::Bob, &exec_tx, &xfer_params, &exec_params, current_slot).await?;

    info!("[Charlie] Executing Dao::Exec tx");
    th.execute_dao_exec_tx(Holder::Charlie, &exec_tx, &xfer_params, &exec_params, current_slot)
        .await?;

    info!("[Rachel] Executing Dao::Exec tx");
    th.execute_dao_exec_tx(Holder::Rachel, &exec_tx, &xfer_params, &exec_params, current_slot)
        .await?;

    info!("[Dao] Executing Dao::Exec tx");
    th.execute_dao_exec_tx(Holder::Dao, &exec_tx, &xfer_params, &exec_params, current_slot).await?;

    th.assert_trees(&HOLDERS);

    // Gather the coins
    th.gather_owncoin(Holder::Dao, xfer_params.outputs[0].clone(), None)?;
    th.gather_owncoin(Holder::Rachel, xfer_params.outputs[1].clone(), None)?;

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
}
