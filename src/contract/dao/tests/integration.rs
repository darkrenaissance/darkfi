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
    crypto::{constants::MERKLE_DEPTH, MerkleNode, TokenId},
    incrementalmerkletree::{bridgetree::BridgeTree, Tree},
    pasta::{group::ff::Field, pallas},
    tx::ContractCall,
};
use darkfi_serial::Encodable;
use log::{debug, info};
use rand::rngs::OsRng;

use darkfi_dao_contract::{
    client::{build_dao_mint_tx, MerkleTree},
    DaoFunction,
};

mod harness;
use harness::{init_logger, DaoTestHarness};

// TODO: Anonymity leaks in this proof of concept:
//
// * Vote updates are linked to the proposal_bulla
// * Nullifier of vote will link vote with the coin when it's spent

// TODO: strategize and cleanup Result/Error usage
// TODO: fix up code doc

// TODO: Commenting this test until it works properly
//#[async_std::test]
async fn integration_test() -> Result<()> {
    init_logger()?;

    let mut th = DaoTestHarness::new().await?;

    // Money parameters
    //let xdrk_supply = 1_000_000;
    //let xrdk_token_id = TokenId::from(pallas::Base::random(&mut OsRng));

    // Governance token parameters
    //let gdrk_supply = 1_000_000;
    let gdrk_token_id = TokenId::from(pallas::Base::random(&mut OsRng));

    // DAO parameters
    let dao_proposer_limit = 110;
    let dao_quorum = 110;
    let dao_approval_ratio_quot = 1;
    let dao_approval_ratio_base = 2;

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
        &th.dao_kp.public,
        dao_bulla_blind,
        &th.dao_kp.secret,
        &th.dao_mint_zkbin,
        &th.dao_mint_pk,
    )?;

    info!("[Alice] ==========================================");
    info!("[Alice] Building Dao::Mint transaction with params");
    info!("[Alice] ==========================================");
    let mut data = vec![DaoFunction::Mint as u8];
    params.encode(&mut data)?;
    let calls = vec![ContractCall { contract_id: th.dao_contract_id, data }];
    let proofs = vec![proofs];
    let mut tx = Transaction { calls, proofs, signatures: vec![] };
    let sigs = tx.create_sigs(&mut OsRng, &[])?;
    tx.signatures = vec![sigs];

    info!("[Alice] ===============================");
    info!("[Alice] Executing Dao::Mint transaction");
    info!("[Alice] ===============================");
    th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;
    // TODO: Witness and add to wallet merkle tree?

    let mut dao_tree = MerkleTree::new(100);
    let dao_leaf_position = {
        let node = MerkleNode::from(params.dao_bulla.inner());
        dao_tree.append(&node);
        dao_tree.witness().unwrap()
    };
    debug!(target: "demo", "Created DAO bulla: {:?}", params.dao_bulla.inner());

    // =======================================================
    // Money::Transfer
    //
    // Mint the initial supply of treasury token
    // and send it all to the DAO directly
    // =======================================================
    debug!(target: "demo", "Stage 2. Minting treasury token");

    Ok(())
}
