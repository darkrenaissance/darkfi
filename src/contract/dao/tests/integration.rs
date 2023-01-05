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

use darkfi::{tx::Transaction, Result};
use darkfi_sdk::{
    crypto::{
        coin::Coin,
        contract_id::{DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
        keypair::Keypair,
        pedersen::pedersen_commitment_u64,
        poseidon_hash, MerkleNode, SecretKey, TokenId,
    },
    incrementalmerkletree::Tree,
    pasta::{
        arithmetic::CurveAffine,
        group::{ff::Field, Curve, Group},
        pallas,
    },
    tx::ContractCall,
};
use darkfi_serial::{Decodable, Encodable};
use log::{debug, info};
use rand::rngs::OsRng;

use darkfi_dao_contract::{
    dao_client,
    dao_client::{
        exec as dao_exec_client,
        mint::{build_dao_mint_tx, MerkleTree, WalletCache},
        propose as dao_propose_client, vote as dao_vote_client,
    },
    money_client, note, DaoFunction,
};

use darkfi_money_contract::{client::EncryptedNote, state::MoneyTransferParams, MoneyFunction};

mod harness;
use harness::{init_logger, DaoTestHarness};

// TODO: Anonymity leaks in this proof of concept:
//
// * Vote updates are linked to the proposal_bulla
// * Nullifier of vote will link vote with the coin when it's spent

// TODO: strategize and cleanup Result/Error usage
// TODO: fix up code doc

#[async_std::test]
async fn integration_test() -> Result<()> {
    init_logger()?;

    let dao_th = DaoTestHarness::new().await?;

    // Money parameters
    let xdrk_supply = 1_000_000;
    let xdrk_token_id = TokenId::from(pallas::Base::random(&mut OsRng));

    // Governance token parameters
    let gdrk_supply = 1_000_000;
    let gdrk_token_id = TokenId::from(pallas::Base::random(&mut OsRng));

    // DAO parameters
    let dao_proposer_limit = 110;
    let dao_quorum = 110;
    let dao_approval_ratio_quot = 1;
    let dao_approval_ratio_base = 2;

    // We use this to receive coins
    let mut cache = WalletCache::new();

    // =======================================================
    // Dao::Mint
    //
    // Create the DAO bulla
    // =======================================================
    debug!(target: "dao", "Stage 1. Creating DAO bulla");

    let dao_bulla_blind = pallas::Base::random(&mut OsRng);

    let (params, proofs) = build_dao_mint_tx(
        dao_proposer_limit,
        dao_quorum,
        dao_approval_ratio_quot,
        dao_approval_ratio_base,
        gdrk_token_id,
        &dao_th.dao_kp.public,
        dao_bulla_blind,
        &dao_th.dao_kp.secret,
        &dao_th.dao_mint_zkbin,
        &dao_th.dao_mint_pk,
    )?;

    let mut data = vec![DaoFunction::Mint as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id: dao_th.dao_contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &[])?;
    tx.signatures = vec![sigs];

    dao_th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    // TODO: Witness and add to wallet merkle tree?

    let mut dao_tree = MerkleTree::new(100);
    let dao_leaf_position = {
        let node = MerkleNode::from(params.dao_bulla.inner());
        dao_tree.append(&node);
        dao_tree.witness().unwrap()
    };
    let dao_bulla = params.dao_bulla;
    debug!(target: "dao", "Created DAO bulla: {:?}", dao_bulla.inner());

    // =======================================================
    // Money::Transfer
    //
    // Mint the initial supply of treasury token
    // and send it all to the DAO directly
    // =======================================================
    debug!(target: "dao", "Stage 2. Minting treasury token");

    cache.track(dao_th.dao_kp.secret);

    // Address of deployed contract in our example is dao::exec::FUNC_ID
    // This field is public, you can see it's being sent to a DAO
    // but nothing else is visible.
    //
    // In the python code we wrote:
    //
    //   spend_hook = b"0xdao_ruleset"
    //
    // TODO: this should be the contract/func ID
    let spend_hook = pallas::Base::from(110);
    // The user_data can be a simple hash of the items passed into the ZK proof
    // up to corresponding linked ZK proof to interpret however they need.
    // In out case, it's the bulla for the DAO
    let user_data = dao_bulla.inner();

    let builder = money_client::Builder {
        clear_inputs: vec![money_client::BuilderClearInputInfo {
            value: xdrk_supply,
            token_id: xdrk_token_id,
            signature_secret: dao_th.faucet_kp.secret,
        }],
        inputs: vec![],
        outputs: vec![money_client::BuilderOutputInfo {
            value: xdrk_supply,
            token_id: xdrk_token_id,
            public: dao_th.dao_kp.public,
            serial: pallas::Base::random(&mut OsRng),
            coin_blind: pallas::Base::random(&mut OsRng),
            spend_hook,
            user_data,
        }],
    };
    let (params, proofs) = builder.build(
        &dao_th.money_mint_zkbin,
        &dao_th.money_mint_pk,
        &dao_th.money_burn_zkbin,
        &dao_th.money_burn_pk,
    )?;

    let contract_id = *MONEY_CONTRACT_ID;

    let mut data = vec![MoneyFunction::Transfer as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &vec![dao_th.faucet_kp.secret])?;
    tx.signatures = vec![sigs];

    dao_th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;

    // Wallet stuff

    // DAO reads the money received from the encrypted note
    {
        assert_eq!(tx.calls.len(), 1);
        let calldata = &tx.calls[0].data;
        let params_data = &calldata[1..];
        let params: MoneyTransferParams = Decodable::decode(params_data)?;

        for output in params.outputs {
            let coin = output.coin;
            let enc_note =
                EncryptedNote { ciphertext: output.ciphertext, ephem_public: output.ephem_public };

            let coin = Coin(coin);
            cache.try_decrypt_note(coin, &enc_note);
        }
    }

    let mut recv_coins = cache.get_received(&dao_th.dao_kp.secret);
    assert_eq!(recv_coins.len(), 1);
    let dao_recv_coin = recv_coins.pop().unwrap();
    let treasury_note = dao_recv_coin.note;

    // Check the actual coin received is valid before accepting it

    let coords = dao_th.dao_kp.public.inner().to_affine().coordinates().unwrap();
    let coin = poseidon_hash::<8>([
        *coords.x(),
        *coords.y(),
        pallas::Base::from(treasury_note.value),
        treasury_note.token_id.inner(),
        treasury_note.serial,
        treasury_note.spend_hook,
        treasury_note.user_data,
        treasury_note.coin_blind,
    ]);
    assert_eq!(coin, dao_recv_coin.coin.0);

    assert_eq!(treasury_note.spend_hook, spend_hook);
    assert_eq!(treasury_note.user_data, dao_bulla.inner());

    debug!(target: "dao", "DAO received a coin worth {} xDRK", treasury_note.value);

    // =======================================================
    // Money::Transfer
    //
    // Mint the governance token
    // Send it to three hodlers
    // =======================================================
    debug!(target: "dao", "Stage 3. Minting governance token");

    cache.track(dao_th.alice_kp.secret);
    cache.track(dao_th.bob_kp.secret);
    cache.track(dao_th.charlie_kp.secret);

    // Spend hook and user data disabled
    let spend_hook = pallas::Base::from(0);
    let user_data = pallas::Base::from(0);

    let output1 = money_client::BuilderOutputInfo {
        value: 400000,
        token_id: gdrk_token_id,
        public: dao_th.alice_kp.public,
        serial: pallas::Base::random(&mut OsRng),
        coin_blind: pallas::Base::random(&mut OsRng),
        spend_hook,
        user_data,
    };

    let output2 = money_client::BuilderOutputInfo {
        value: 400000,
        token_id: gdrk_token_id,
        public: dao_th.bob_kp.public,
        serial: pallas::Base::random(&mut OsRng),
        coin_blind: pallas::Base::random(&mut OsRng),
        spend_hook,
        user_data,
    };

    let output3 = money_client::BuilderOutputInfo {
        value: 200000,
        token_id: gdrk_token_id,
        public: dao_th.charlie_kp.public,
        serial: pallas::Base::random(&mut OsRng),
        coin_blind: pallas::Base::random(&mut OsRng),
        spend_hook,
        user_data,
    };

    assert!(2 * 400000 + 200000 == gdrk_supply);

    let builder = money_client::Builder {
        clear_inputs: vec![money_client::BuilderClearInputInfo {
            value: gdrk_supply,
            token_id: gdrk_token_id,
            // This might be different for various tokens but lets reuse it here
            signature_secret: dao_th.faucet_kp.secret,
        }],
        inputs: vec![],
        outputs: vec![output1, output2, output3],
    };
    let (params, proofs) = builder.build(
        &dao_th.money_mint_zkbin,
        &dao_th.money_mint_pk,
        &dao_th.money_burn_zkbin,
        &dao_th.money_burn_pk,
    )?;

    let contract_id = *MONEY_CONTRACT_ID;

    let mut data = vec![MoneyFunction::Transfer as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &vec![dao_th.faucet_kp.secret])?;
    tx.signatures = vec![sigs];

    dao_th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;

    // Wallet
    {
        assert_eq!(tx.calls.len(), 1);
        let calldata = &tx.calls[0].data;
        let params_data = &calldata[1..];
        let params: MoneyTransferParams = Decodable::decode(params_data)?;

        for output in params.outputs {
            let coin = output.coin;
            let enc_note =
                EncryptedNote { ciphertext: output.ciphertext, ephem_public: output.ephem_public };
            let coin = Coin(coin);
            cache.try_decrypt_note(coin, &enc_note);
        }
    }

    let gov_keypairs = vec![dao_th.alice_kp, dao_th.bob_kp, dao_th.charlie_kp];
    let mut gov_recv = vec![None, None, None];
    // Check that each person received one coin
    for (i, key) in gov_keypairs.iter().enumerate() {
        let gov_recv_coin = {
            let mut recv_coins = cache.get_received(&key.secret);
            assert_eq!(recv_coins.len(), 1);
            let recv_coin = recv_coins.pop().unwrap();
            let note = &recv_coin.note;

            assert_eq!(note.token_id, gdrk_token_id);
            // Normal payment
            assert_eq!(note.spend_hook, pallas::Base::from(0));
            assert_eq!(note.user_data, pallas::Base::from(0));

            let (pub_x, pub_y) = key.public.xy();
            let coin = poseidon_hash::<8>([
                pub_x,
                pub_y,
                pallas::Base::from(note.value),
                note.token_id.inner(),
                note.serial,
                note.spend_hook,
                note.user_data,
                note.coin_blind,
            ]);
            assert_eq!(coin, recv_coin.coin.0);

            debug!(target: "dao", "Holder{} received a coin worth {} gDRK", i, note.value);

            recv_coin
        };
        gov_recv[i] = Some(gov_recv_coin);
    }
    // unwrap them for this demo
    let gov_recv: Vec<_> = gov_recv.into_iter().map(|r| r.unwrap()).collect();

    // =======================================================
    // Dao::Propose
    //
    // Propose the vote
    // In order to make a valid vote, first the proposer must
    // meet a criteria for a minimum number of gov tokens
    //
    // DAO rules:
    // 1. gov token IDs must match on all inputs
    // 2. proposals must be submitted by minimum amount
    // 3. all votes >= quorum
    // 4. outcome > approval_ratio
    // 5. structure of outputs
    //   output 0: value and address
    //   output 1: change address
    // =======================================================
    debug!(target: "dao", "Stage 4. Propose the vote");

    // TODO: look into proposal expiry once time for voting has finished

    let receiver_keypair = Keypair::random(&mut OsRng);

    let (money_leaf_position, money_merkle_path) = {
        let tree = &cache.tree;
        let leaf_position = gov_recv[0].leaf_position;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    // TODO: is it possible for an invalid transfer() to be constructed on exec()?
    //       need to look into this
    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao_client::ProposalStakeInput {
        secret: dao_th.alice_kp.secret,
        note: gov_recv[0].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let (dao_merkle_path, dao_merkle_root) = {
        let tree = &dao_tree;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(dao_leaf_position, &root).unwrap();
        (merkle_path, root)
    };

    let dao_params = dao_client::Dao {
        proposer_limit: dao_proposer_limit,
        quorum: dao_quorum,
        approval_ratio_base: dao_approval_ratio_base,
        approval_ratio_quot: dao_approval_ratio_quot,
        gov_token_id: gdrk_token_id,
        public_key: dao_th.dao_kp.public,
        bulla_blind: dao_bulla_blind,
    };

    let proposal = dao_client::Proposal {
        dest: receiver_keypair.public,
        amount: 1000,
        serial: pallas::Base::random(&mut OsRng),
        token_id: xdrk_token_id,
        blind: pallas::Base::random(&mut OsRng),
    };

    let call = dao_client::ProposeCall {
        inputs: vec![input],
        proposal,
        dao: dao_params.clone(),
        dao_leaf_position,
        dao_merkle_path,
        dao_merkle_root,
    };
    let (params, proofs) = call.make(
        &dao_th.dao_propose_burn_zkbin,
        &dao_th.dao_propose_burn_pk,
        &dao_th.dao_propose_main_zkbin,
        &dao_th.dao_propose_main_pk,
    )?;

    let contract_id = *DAO_CONTRACT_ID;

    let mut data = vec![DaoFunction::Propose as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &vec![signature_secret])?;
    tx.signatures = vec![sigs];

    dao_th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;

    //// Wallet

    // Read received proposal
    let (proposal, proposal_bulla) = {
        // TODO: EncryptedNote should be accessible by wasm and put in the structs directly
        let enc_note = note::EncryptedNote2 {
            ciphertext: params.ciphertext,
            ephem_public: params.ephem_public,
        };
        let note: dao_propose_client::Note = enc_note.decrypt(&dao_th.dao_kp.secret).unwrap();

        // TODO: check it belongs to DAO bulla

        // Return the proposal info
        (note.proposal, params.proposal_bulla)
    };
    debug!(target: "dao", "Proposal now active!");
    debug!(target: "dao", "  destination: {:?}", proposal.dest);
    debug!(target: "dao", "  amount: {}", proposal.amount);
    debug!(target: "dao", "  token_id: {:?}", proposal.token_id);
    debug!(target: "dao", "  dao_bulla: {:?}", dao_bulla.inner());
    debug!(target: "dao", "Proposal bulla: {:?}", proposal_bulla);

    // =======================================================
    // Proposal is accepted!
    // Start the voting
    // =======================================================

    // Copying these schizo comments from python code:
    // Lets the voting begin
    // Voters have access to the proposal and dao data
    //   vote_state = VoteState()
    // We don't need to copy nullifier set because it is checked from gov_state
    // in vote_state_transition() anyway
    //
    // TODO: what happens if voters don't unblind their vote
    // Answer:
    //   1. there is a time limit
    //   2. both the MPC or users can unblind
    //
    // TODO: bug if I vote then send money, then we can double vote
    // TODO: all timestamps missing
    //       - timelock (future voting starts in 2 days)
    // Fix: use nullifiers from money gov state only from
    // beginning of gov period
    // Cannot use nullifiers from before voting period

    debug!(target: "dao", "Stage 5. Start voting");

    // We were previously saving updates here for testing
    // let mut updates = vec![];

    // User 1: YES

    let (money_leaf_position, money_merkle_path) = {
        let tree = &cache.tree;
        let leaf_position = gov_recv[0].leaf_position;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao_vote_client::BuilderInput {
        secret: dao_th.alice_kp.secret,
        note: gov_recv[0].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let vote_option: bool = true;
    // assert!(vote_option || !vote_option); // wtf

    // We create a new keypair to encrypt the vote.
    // For the demo MVP, you can just use the dao_keypair secret
    let vote_keypair_1 = Keypair::random(&mut OsRng);

    let builder = dao_vote_client::Builder {
        inputs: vec![input],
        vote: dao_vote_client::Vote {
            vote_option,
            vote_option_blind: pallas::Scalar::random(&mut OsRng),
        },
        vote_keypair: vote_keypair_1,
        proposal: proposal.clone(),
        dao: dao_params.clone(),
    };
    let (params, proofs) = builder.build(
        &dao_th.dao_vote_burn_zkbin,
        &dao_th.dao_vote_burn_pk,
        &dao_th.dao_vote_main_zkbin,
        &dao_th.dao_vote_main_pk,
    )?;

    let contract_id = *DAO_CONTRACT_ID;

    let mut data = vec![DaoFunction::Vote as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &vec![signature_secret])?;
    tx.signatures = vec![sigs];

    dao_th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;

    // Secret vote info. Needs to be revealed at some point.
    // TODO: look into verifiable encryption for notes
    // TODO: look into timelock puzzle as a possibility
    let vote_note_1 = {
        // TODO: EncryptedNote should be accessible by wasm and put in the structs directly
        let enc_note = note::EncryptedNote2 {
            ciphertext: params.ciphertext,
            ephem_public: params.ephem_public,
        };
        let note: dao_vote_client::Note = enc_note.decrypt(&vote_keypair_1.secret).unwrap();
        note
    };
    debug!(target: "dao", "User 1 voted!");
    debug!(target: "dao", "  vote_option: {}", vote_note_1.vote.vote_option);
    debug!(target: "dao", "  value: {}", vote_note_1.vote_value);

    // User 2: NO

    let (money_leaf_position, money_merkle_path) = {
        let tree = &cache.tree;
        let leaf_position = gov_recv[1].leaf_position;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao_vote_client::BuilderInput {
        //secret: gov_keypair_2.secret,
        secret: dao_th.bob_kp.secret,
        note: gov_recv[1].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let vote_option: bool = false;
    // assert!(vote_option || !vote_option); // wtf

    // We create a new keypair to encrypt the vote.
    let vote_keypair_2 = Keypair::random(&mut OsRng);

    let builder = dao_vote_client::Builder {
        inputs: vec![input],
        vote: dao_vote_client::Vote {
            vote_option,
            vote_option_blind: pallas::Scalar::random(&mut OsRng),
        },
        vote_keypair: vote_keypair_2,
        proposal: proposal.clone(),
        dao: dao_params.clone(),
    };
    let (params, proofs) = builder.build(
        &dao_th.dao_vote_burn_zkbin,
        &dao_th.dao_vote_burn_pk,
        &dao_th.dao_vote_main_zkbin,
        &dao_th.dao_vote_main_pk,
    )?;

    let contract_id = *DAO_CONTRACT_ID;

    let mut data = vec![DaoFunction::Vote as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &vec![signature_secret])?;
    tx.signatures = vec![sigs];

    dao_th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;

    let vote_note_2 = {
        // TODO: EncryptedNote should be accessible by wasm and put in the structs directly
        let enc_note = note::EncryptedNote2 {
            ciphertext: params.ciphertext,
            ephem_public: params.ephem_public,
        };
        let note: dao_vote_client::Note = enc_note.decrypt(&vote_keypair_2.secret).unwrap();
        note
    };
    debug!(target: "dao", "User 2 voted!");
    debug!(target: "dao", "  vote_option: {}", vote_note_2.vote.vote_option);
    debug!(target: "dao", "  value: {}", vote_note_2.vote_value);

    // User 3: YES

    let (money_leaf_position, money_merkle_path) = {
        let tree = &cache.tree;
        let leaf_position = gov_recv[2].leaf_position;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let signature_secret = SecretKey::random(&mut OsRng);
    let input = dao_vote_client::BuilderInput {
        //secret: gov_keypair_3.secret,
        secret: dao_th.charlie_kp.secret,
        note: gov_recv[2].note.clone(),
        leaf_position: money_leaf_position,
        merkle_path: money_merkle_path,
        signature_secret,
    };

    let vote_option: bool = true;
    // assert!(vote_option || !vote_option); // wtf

    // We create a new keypair to encrypt the vote.
    let vote_keypair_3 = Keypair::random(&mut OsRng);

    let builder = dao_vote_client::Builder {
        inputs: vec![input],
        vote: dao_vote_client::Vote {
            vote_option,
            vote_option_blind: pallas::Scalar::random(&mut OsRng),
        },
        vote_keypair: vote_keypair_3,
        proposal: proposal.clone(),
        dao: dao_params.clone(),
    };
    let (params, proofs) = builder.build(
        &dao_th.dao_vote_burn_zkbin,
        &dao_th.dao_vote_burn_pk,
        &dao_th.dao_vote_main_zkbin,
        &dao_th.dao_vote_main_pk,
    )?;

    let contract_id = *DAO_CONTRACT_ID;

    let mut data = vec![DaoFunction::Vote as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &vec![signature_secret])?;
    tx.signatures = vec![sigs];

    dao_th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;

    // Secret vote info. Needs to be revealed at some point.
    // TODO: look into verifiable encryption for notes
    // TODO: look into timelock puzzle as a possibility
    let vote_note_3 = {
        // TODO: EncryptedNote should be accessible by wasm and put in the structs directly
        let enc_note = note::EncryptedNote2 {
            ciphertext: params.ciphertext,
            ephem_public: params.ephem_public,
        };
        let note: dao_vote_client::Note = enc_note.decrypt(&vote_keypair_3.secret).unwrap();
        note
    };
    debug!(target: "dao", "User 3 voted!");
    debug!(target: "dao", "  vote_option: {}", vote_note_3.vote.vote_option);
    debug!(target: "dao", "  value: {}", vote_note_3.vote_value);

    // Every votes produces a semi-homomorphic encryption of their vote.
    // Which is either yes or no
    // We copy the state tree for the governance token so coins can be used
    // to vote on other proposals at the same time.
    // With their vote, they produce a ZK proof + nullifier
    // The votes are unblinded by MPC to a selected party at the end of the
    // voting period.
    // (that's if we want votes to be hidden during voting)

    let mut yes_votes_value = 0;
    let mut yes_votes_blind = pallas::Scalar::from(0);
    let mut yes_votes_commit = pallas::Point::identity();

    let mut all_votes_value = 0;
    let mut all_votes_blind = pallas::Scalar::from(0);
    let mut all_votes_commit = pallas::Point::identity();

    // We were previously saving votes to a Vec<Update> for testing.
    // However since Update is now UpdateBase it gets moved into update.apply().
    // So we need to think of another way to run these tests.
    //assert!(updates.len() == 3);

    for (i, note /* update*/) in [vote_note_1, vote_note_2, vote_note_3]
        .iter() /*.zip(updates)*/
        .enumerate()
    {
        let vote_commit = pedersen_commitment_u64(note.vote_value, note.vote_value_blind);
        //assert!(update.value_commit == all_vote_value_commit);
        all_votes_commit += vote_commit;
        all_votes_blind += note.vote_value_blind;

        let yes_vote_commit = pedersen_commitment_u64(
            note.vote.vote_option as u64 * note.vote_value,
            note.vote.vote_option_blind,
        );
        //assert!(update.yes_vote_commit == yes_vote_commit);

        yes_votes_commit += yes_vote_commit;
        yes_votes_blind += note.vote.vote_option_blind;

        let vote_option = note.vote.vote_option;

        if vote_option {
            yes_votes_value += note.vote_value;
        }
        all_votes_value += note.vote_value;
        let vote_result: String = if vote_option { "yes".to_string() } else { "no".to_string() };

        debug!(target: "dao", "Voter {} voted {}", i, vote_result);
    }

    debug!(target: "dao", "Outcome = {} / {}", yes_votes_value, all_votes_value);

    assert!(all_votes_commit == pedersen_commitment_u64(all_votes_value, all_votes_blind));
    assert!(yes_votes_commit == pedersen_commitment_u64(yes_votes_value, yes_votes_blind));

    // =======================================================
    // Execute the vote
    // =======================================================

    debug!(target: "dao", "Stage 6. Execute vote");

    // Used to export user_data from this coin so it can be accessed by DAO::exec()
    let user_data_blind = pallas::Base::random(&mut OsRng);

    let user_serial = pallas::Base::random(&mut OsRng);
    let user_coin_blind = pallas::Base::random(&mut OsRng);
    let dao_serial = pallas::Base::random(&mut OsRng);
    let dao_coin_blind = pallas::Base::random(&mut OsRng);
    let input_value = treasury_note.value;
    let input_value_blind = pallas::Scalar::random(&mut OsRng);
    let tx_signature_secret = SecretKey::random(&mut OsRng);
    let exec_signature_secret = SecretKey::random(&mut OsRng);

    let (treasury_leaf_position, treasury_merkle_path) = {
        let tree = &cache.tree;
        let leaf_position = dao_recv_coin.leaf_position;
        let root = tree.root(0).unwrap();
        let merkle_path = tree.authentication_path(leaf_position, &root).unwrap();
        (leaf_position, merkle_path)
    };

    let input = money_client::BuilderInputInfo {
        leaf_position: treasury_leaf_position,
        merkle_path: treasury_merkle_path,
        secret: dao_th.dao_kp.secret,
        note: treasury_note,
        user_data_blind,
        value_blind: input_value_blind,
        signature_secret: tx_signature_secret,
    };

    // TODO: this should be the contract/func ID
    //let spend_hook = pallas::Base::from(110);
    let spend_hook = DAO_CONTRACT_ID.inner();
    // The user_data can be a simple hash of the items passed into the ZK proof
    // up to corresponding linked ZK proof to interpret however they need.
    // In out case, it's the bulla for the DAO
    let user_data = dao_bulla.inner();

    let builder = money_client::Builder {
        clear_inputs: vec![],
        inputs: vec![input],
        outputs: vec![
            // Sending money
            money_client::BuilderOutputInfo {
                value: 1000,
                token_id: xdrk_token_id,
                //public: user_keypair.public,
                public: receiver_keypair.public,
                serial: proposal.serial,
                coin_blind: proposal.blind,
                spend_hook: pallas::Base::from(0),
                user_data: pallas::Base::from(0),
            },
            // Change back to DAO
            money_client::BuilderOutputInfo {
                value: xdrk_supply - 1000,
                token_id: xdrk_token_id,
                public: dao_th.dao_kp.public,
                serial: dao_serial,
                coin_blind: dao_coin_blind,
                spend_hook,
                user_data,
            },
        ],
    };
    let (xfer_params, xfer_proofs) = builder.build(
        &dao_th.money_mint_zkbin,
        &dao_th.money_mint_pk,
        &dao_th.money_burn_zkbin,
        &dao_th.money_burn_pk,
    )?;

    let mut data = vec![MoneyFunction::Transfer as u8];
    xfer_params.encode(&mut data)?;
    let xfer_call = ContractCall { contract_id: *MONEY_CONTRACT_ID, data };

    let builder = dao_exec_client::Builder {
        proposal,
        dao: dao_params.clone(),
        yes_votes_value,
        all_votes_value,
        yes_votes_blind,
        all_votes_blind,
        user_serial,
        user_coin_blind,
        dao_serial,
        dao_coin_blind,
        input_value,
        input_value_blind,
        hook_dao_exec: spend_hook,
        signature_secret: exec_signature_secret,
    };
    let (exec_params, exec_proofs) = builder.build(&dao_th.dao_exec_zkbin, &dao_th.dao_exec_pk)?;

    let mut data = vec![DaoFunction::Exec as u8];
    exec_params.encode(&mut data)?;
    let exec_call = ContractCall { contract_id: *DAO_CONTRACT_ID, data };

    let calls = vec![xfer_call, exec_call];
    let proofs = vec![xfer_proofs, exec_proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let xfer_sigs = tx.create_sigs(&mut OsRng, &vec![tx_signature_secret])?;
    let exec_sigs = tx.create_sigs(&mut OsRng, &vec![exec_signature_secret])?;
    tx.signatures = vec![xfer_sigs, exec_sigs];

    dao_th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;

    Ok(())
}
