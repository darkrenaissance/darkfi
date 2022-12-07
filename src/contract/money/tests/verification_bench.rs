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

use std::{env, str::FromStr};

use darkfi::{tx::Transaction, Result};
use darkfi_sdk::{
    crypto::{poseidon_hash, ContractId, MerkleNode, Nullifier, TokenId},
    incrementalmerkletree::Tree,
    pasta::{group::ff::Field, pallas},
    tx::ContractCall,
};
use darkfi_serial::Encodable;
use log::info;
use rand::{rngs::OsRng, Rng};

use darkfi_money_contract::{
    client::{build_transfer_tx, Coin, EncryptedNote, OwnCoin},
    MoneyFunction,
};

mod harness;
use harness::{init_logger, MoneyTestHarness};

#[async_std::test]
async fn alice2alice_random_amounts() -> Result<()> {
    init_logger()?;

    const ALICE_AIRDROP: u64 = 1000;

    // n transactions to loop
    let mut n = 3;
    for arg in env::args() {
        match usize::from_str(&arg) {
            Ok(v) => {
                n = v;
                break
            }
            Err(_) => continue,
        };
    }

    let mut th = MoneyTestHarness::new().await?;
    let token_id = TokenId::from(pallas::Base::random(&mut OsRng));
    let contract_id = ContractId::from(pallas::Base::from(u64::MAX - 420));

    let mut owncoins = vec![];

    let (airdrop_tx, airdrop_params) = th.airdrop(ALICE_AIRDROP, token_id, &th.alice_kp.public)?;

    th.faucet_state.read().await.verify_transactions(&[airdrop_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(airdrop_params.outputs[0].coin));

    th.alice_state.read().await.verify_transactions(&[airdrop_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(airdrop_params.outputs[0].coin));
    let leaf_position = th.alice_merkle_tree.witness().unwrap();

    let ciphertext = airdrop_params.outputs[0].ciphertext.clone();
    let ephem_public = airdrop_params.outputs[0].ephem_public;
    let e_note = EncryptedNote { ciphertext, ephem_public };
    let note = e_note.decrypt(&th.alice_kp.secret)?;

    owncoins.push(OwnCoin {
        coin: Coin::from(airdrop_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice_kp.secret,
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
        leaf_position,
    });

    for i in 0..n {
        info!("Building Alice2Alice transfer tx {}", i);
        
        info!("Alice coins: {}", owncoins.len());
        for (i, c) in owncoins.iter().enumerate() {
            info!("\t coin {} value: {}", i, c.note.value);
        }

        let amount = rand::thread_rng().gen_range(1..ALICE_AIRDROP);
        info!("Sending: {}", amount);

        let (params, proofs, secret_keys, spent_coins) = build_transfer_tx(
            &th.alice_kp,
            &th.alice_kp.public,
            amount,
            token_id,
            &owncoins,
            &th.alice_merkle_tree,
            &th.mint_zkbin,
            &th.mint_pk,
            &th.burn_zkbin,
            &th.burn_pk,
            false,
        )?;

        let mut data = vec![MoneyFunction::Transfer as u8];
        params.encode(&mut data)?;
        let calls = vec![ContractCall { contract_id, data }];
        let proofs = vec![proofs];
        let mut tx = Transaction { calls, proofs, signatures: vec![] };
        let sigs = tx.create_sigs(&mut OsRng, &secret_keys)?;
        tx.signatures = vec![sigs];

        // Remove the owncoins we've spent
        for spent in spent_coins {
            owncoins.retain(|x| x != &spent);
        }

        // Apply the state transition
        th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;

        // Gather new owncoins
        for output in params.outputs {
            let e_note = EncryptedNote {
                ciphertext: output.ciphertext.clone(),
                ephem_public: output.ephem_public,
            };
            let note = e_note.decrypt(&th.alice_kp.secret)?;

            th.alice_merkle_tree.append(&MerkleNode::from(output.coin));
            let leaf_position = th.alice_merkle_tree.witness().unwrap();

            let owncoin = OwnCoin {
                coin: Coin::from(output.coin),
                note: note.clone(),
                secret: th.alice_kp.secret,
                nullifier: Nullifier::from(poseidon_hash([
                    th.alice_kp.secret.inner(),
                    note.serial,
                ])),
                leaf_position,
            };

            owncoins.push(owncoin);
        }
    }

    Ok(())
}
