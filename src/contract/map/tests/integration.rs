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


use std::time::Instant;
use darkfi::Result;
use darkfi_sdk::{
    crypto::{
        poseidon_hash,
        Keypair,
        MerkleNode,
        Nullifier,
        MAP_CONTRACT_ID
    },
    incrementalmerkletree::Tree,
    pasta::pallas,
    // db::{db_lookup, db_get} link error?
};
use log::{info, debug};
use rand::rngs::OsRng;
use darkfi_map_contract::MAP_CONTRACT_ENTRIES_TREE;
use darkfi_serial::{deserialize, serialize};

mod harness;
use harness::{init_logger, MapTestHarness};

#[async_std::test]
async fn map_integration() -> Result<()> {
    let current_slot = 0;

    init_logger();

    let mut th = MapTestHarness::new().await?;
    let (alice_tx, alice_params) = th.set(
        th.alice.keypair.secret,
        pallas::Base::from(1), // lock
        pallas::Base::from(1), // car
        pallas::Base::from(2), // key
        pallas::Base::from(4), // value
    )?;

    info!(target: "map", "[Faucet] =============================");
    info!(target: "map", "[Faucet] Executing Alice set tx");
    info!(target: "map", "[Faucet] =============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .faucet
        .state
        .read()
        .await
        .verify_transactions(&[alice_tx.clone()], current_slot, true)
        .await?;
    assert!(erroneous_txs.is_empty());

    info!(target: "map", "[Alice] =============================");
    info!(target: "map", "[Alice] Executing Alice set tx");
    info!(target: "map", "[Alice] =============================");
    let timer = Instant::now();
    let erroneous_txs = th
        .alice
        .state
        .read()
        .await
        .verify_transactions(&[alice_tx.clone()], current_slot, true)
        .await?;
    debug!("error_tx: {:?}", erroneous_txs);
    assert!(erroneous_txs.is_empty());

    // let slot = poseidon_hash([alice_params.account, alice_params.key]);
    // let db   = db_lookup(*MAP_CONTRACT_ID, MAP_CONTRACT_ENTRIES_TREE)?;
    // db_get(db, &serialize(&slot))?;
    // match db_get(db, &serialize(&slot))? {
    //     None => panic!("slot should be set"),
    //     Some(locked) => {
    //         let lock: pallas::Base = deserialize(&locked)?;
    //         assert!(lock == pallas::Base::one());
    //     }
    // };
    // match db_get(db, &serialize(&(slot.add(&pallas::Base::one()))))? {
    //     None => panic!("slot + 1 should be set"),
    //     Some(value) => {
    //         let value: pallas::Base = deserialize(&value)?;
    //         assert!(value == pallas::Base::from(4));
    //     }
    // };

    Ok(())
}
