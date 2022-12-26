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
        coin::Coin, constants::MERKLE_DEPTH, contract_id::MONEY_CONTRACT_ID, poseidon_hash,
        MerkleNode, TokenId,
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
    money_client, DaoFunction,
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

// TODO: Commenting this test until it works properly
#[async_std::test]
async fn integration_test() -> Result<()> {
    init_logger()?;

    let mut dao_th = DaoTestHarness::new().await?;
    let mut money_th = MoneyTestHarness::new().await?;

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

    // We use this to receive coins
    //let mut cache = WalletCache::new();

    //let (params, proofs) = builder.build(&zk_bins)?;
    // TODO: this should be the contract/func ID
    let spend_hook = pallas::Base::from(110);
    let user_data = dao_bulla.inner();

    let builder = money_client::Builder {
        clear_inputs: vec![money_client::BuilderClearInputInfo {
            value: xdrk_supply,
            token_id: xdrk_token_id,
            signature_secret: money_th.faucet_kp.secret,
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
        &money_th.mint_zkbin,
        &money_th.mint_pk,
        &money_th.burn_zkbin,
        &money_th.burn_pk,
    )?;

    let contract_id = *MONEY_CONTRACT_ID;

    let mut data = vec![MoneyFunction::Transfer as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &vec![money_th.faucet_kp.secret])?;
    tx.signatures = vec![sigs];

    money_th.faucet_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    money_th.faucet_merkle_tree.append(&MerkleNode::from(params.outputs[0].coin));

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

    ///////////////////////////////////////////////////

    Ok(())
}
