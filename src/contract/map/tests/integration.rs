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
    crypto::{poseidon_hash, Keypair, MerkleNode, Nullifier, SecretKey, MAP_CONTRACT_ID},
    incrementalmerkletree::Tree,
    pasta::pallas,
};
use log::{info, debug};
use rand::rngs::OsRng;
use darkfi_map_contract::MAP_CONTRACT_ENTRIES_TREE;
use darkfi_serial::deserialize;

mod harness;
use harness::{init_logger, MapTestHarness};

#[async_std::test]
async fn map_integration() -> Result<()> {
    let current_slot = 0;

    init_logger();

    let mut th = MapTestHarness::new().await?;
    let (alice_tx, alice_params) = th.set(
        th.alice.keypair.secret,
        pallas::Base::from(2),
        pallas::Base::from(4),
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
    assert!(erroneous_txs.is_empty());

    // let db = db_lookup(*MAP_CONTRACT_ID, MAP_CONTRACT_ENTRIES_TREE)?;
    // poesidon_hash(account_id, 2): 0x25e70cb58fb267452c84b78df9090b516901ff822dbcde7b4e83575732d77390
    // db_get ....

    Ok(())
}
