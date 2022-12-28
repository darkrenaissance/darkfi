/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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
        constants::MERKLE_DEPTH,
        contract_id::{DAO_CONTRACT_ID, MONEY_CONTRACT_ID},
        keypair::Keypair,
        poseidon_hash, MerkleNode, SecretKey, TokenId,
    },
    incrementalmerkletree::{bridgetree::BridgeTree, Tree},
    pasta::{
        arithmetic::CurveAffine,
        group::{ff::Field, Curve},
        pallas,
    },
    tx::ContractCall,
};
use darkfi_serial::{Decodable, Encodable};
use log::{debug, info};
use rand::rngs::OsRng;

use darkfi_dao_contract::{
    dao_client::{build_dao_mint_tx, MerkleTree, WalletCache},
    dao_propose_client, money_client, note, DaoFunction,
};

use darkfi_money_contract::{
    client::{build_half_swap_tx, build_transfer_tx, EncryptedNote, OwnCoin},
    state::MoneyTransferParams,
    MoneyFunction,
};

mod dao_harness;
use dao_harness::DaoTestHarness;

mod money_harness;
use money_harness::{init_logger, MoneyTestHarness};

// TODO: Anonymity leaks in this proof of concept:
//
// * Vote updates are linked to the proposal_bulla
// * Nullifier of vote will link vote with the coin when it's spent

// TODO: strategize and cleanup Result/Error usage
// TODO: fix up code doc

#[async_std::test]
async fn integration_test() -> Result<()> {
    init_logger()?;

    let mut dao_th = DaoTestHarness::new().await?;

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
    debug!(target: "demo", "Stage 1. Creating DAO bulla");

    let dao_bulla_blind = pallas::Base::random(&mut OsRng);

    info!("[Alice] =========================");
    info!("[Alice] Building Dao::Mint params");
    info!("[Alice] =========================");
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

    info!("[Alice] ==========================================");
    info!("[Alice] Building Dao::Mint transaction with params");
    info!("[Alice] ==========================================");
    let mut data = vec![DaoFunction::Mint as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id: dao_th.dao_contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &[])?;
    tx.signatures = vec![sigs];

    info!("[Alice] ===============================");
    info!("[Alice] Executing Dao::Mint transaction");
    info!("[Alice] ===============================");
    dao_th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    // TODO: Witness and add to wallet merkle tree?

    let mut dao_tree = MerkleTree::new(100);
    let dao_leaf_position = {
        let node = MerkleNode::from(params.dao_bulla.inner());
        dao_tree.append(&node);
        dao_tree.witness().unwrap()
    };
    let dao_bulla = params.dao_bulla;
    debug!(target: "demo", "Created DAO bulla: {:?}", dao_bulla.inner());

    // =======================================================
    // Money::Transfer
    //
    // Mint the initial supply of treasury token
    // and send it all to the DAO directly
    // =======================================================
    debug!(target: "demo", "Stage 2. Minting treasury token");

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

    debug!("DAO received a coin worth {} xDRK", treasury_note.value);

    // =======================================================
    // Money::Transfer
    //
    // Mint the governance token
    // Send it to three hodlers
    // =======================================================
    debug!(target: "demo", "Stage 3. Minting governance token");

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

            debug!("Holder{} received a coin worth {} gDRK", i, note.value);

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
    debug!(target: "demo", "Stage 4. Propose the vote");

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
    let input = dao_propose_client::BuilderInput {
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

    let dao_params = dao_propose_client::DaoParams {
        proposer_limit: dao_proposer_limit,
        quorum: dao_quorum,
        approval_ratio_base: dao_approval_ratio_base,
        approval_ratio_quot: dao_approval_ratio_quot,
        gov_token_id: gdrk_token_id,
        public_key: dao_th.dao_kp.public,
        bulla_blind: dao_bulla_blind,
    };

    let proposal = dao_propose_client::Proposal {
        dest: receiver_keypair.public,
        amount: 1000,
        serial: pallas::Base::random(&mut OsRng),
        token_id: xdrk_token_id,
        blind: pallas::Base::random(&mut OsRng),
    };

    let builder = dao_propose_client::Builder {
        inputs: vec![input],
        proposal,
        dao: dao_params.clone(),
        dao_leaf_position,
        dao_merkle_path,
        dao_merkle_root,
    };
    let (params, proofs) = builder.build(
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
    debug!(target: "demo", "Proposal now active!");
    debug!(target: "demo", "  destination: {:?}", proposal.dest);
    debug!(target: "demo", "  amount: {}", proposal.amount);
    debug!(target: "demo", "  token_id: {:?}", proposal.token_id);
    debug!(target: "demo", "  dao_bulla: {:?}", dao_bulla.inner());
    debug!(target: "demo", "Proposal bulla: {:?}", proposal_bulla);

    Ok(())
}
