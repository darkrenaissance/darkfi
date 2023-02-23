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

use std::{env, str::FromStr};

use darkfi::{tx::Transaction, Result};
use darkfi_sdk::{
    crypto::{
        merkle_prelude::*, pallas, pasta_prelude::*, poseidon_hash, Coin, MerkleNode, Nullifier,
        TokenId, MONEY_CONTRACT_ID,
    },
    ContractCall,
};
use darkfi_serial::Encodable;
use log::info;
use rand::{prelude::IteratorRandom, rngs::OsRng, Rng};

use darkfi_money_contract::{
    client::{transfer_v1::TransferCallBuilder, MoneyNote, OwnCoin},
    MoneyFunction::TransferV1 as MoneyTransfer,
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

    // Initialize harness
    let mut th = MoneyTestHarness::new().await?;

    info!(target: "money", "[Faucet] ===================================================");
    info!(target: "money", "[Faucet] Building Money::Transfer params for Alice's airdrop");
    info!(target: "money", "[Faucet] ===================================================");
    let contract_id = *MONEY_CONTRACT_ID;
    let (airdrop_tx, airdrop_params) = th.airdrop(ALICE_AIRDROP, th.alice_kp.public)?;

    info!(target: "money", "[Faucet] ==========================");
    info!(target: "money", "[Faucet] Executing Alice airdrop tx");
    info!(target: "money", "[Faucet] ==========================");
    th.faucet_state.read().await.verify_transactions(&[airdrop_tx.clone()], true).await?;
    th.faucet_merkle_tree.append(&MerkleNode::from(airdrop_params.outputs[0].coin.inner()));
    info!(target: "money", "[Alice] ==========================");
    info!(target: "money", "[Alice] Executing Alice airdrop tx");
    info!(target: "money", "[Alice] ==========================");
    th.alice_state.read().await.verify_transactions(&[airdrop_tx.clone()], true).await?;
    th.alice_merkle_tree.append(&MerkleNode::from(airdrop_params.outputs[0].coin.inner()));

    assert!(th.faucet_merkle_tree.root(0).unwrap() == th.alice_merkle_tree.root(0).unwrap());

    // Gather new owncoins
    let mut owncoins = vec![];
    let leaf_position = th.alice_merkle_tree.witness().unwrap();
    let note: MoneyNote = airdrop_params.outputs[0].note.decrypt(&th.alice_kp.secret)?;
    let token_id = note.token_id;
    owncoins.push(OwnCoin {
        coin: Coin::from(airdrop_params.outputs[0].coin),
        note: note.clone(),
        secret: th.alice_kp.secret,
        nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
        leaf_position,
    });

    // Execute transactions loop
    for i in 0..n {
        info!(target: "money", "[Alice] ===============================================");
        info!(target: "money", "[Alice] Building Money::Transfer params for transfer {}", i);
        info!(target: "money", "[Alice] Alice coins: {}", owncoins.len());
        for (i, c) in owncoins.iter().enumerate() {
            info!(target: "money", "[Alice] \t coin {} value: {}", i, c.note.value);
        }
        let amount = rand::thread_rng().gen_range(1..ALICE_AIRDROP);
        info!(target: "money", "[Alice] Sending: {}", amount);
        info!(target: "money", "[Alice] ===============================================");
        let call_debris = TransferCallBuilder {
            keypair: th.alice_kp,
            recipient: th.alice_kp.public,
            value: amount,
            token_id,
            rcpt_spend_hook: pallas::Base::zero(),
            rcpt_user_data: pallas::Base::zero(),
            rcpt_user_data_blind: pallas::Base::random(&mut OsRng),
            change_spend_hook: pallas::Base::zero(),
            change_user_data: pallas::Base::zero(),
            change_user_data_blind: pallas::Base::random(&mut OsRng),
            coins: owncoins.clone(),
            tree: th.alice_merkle_tree.clone(),
            mint_zkbin: th.mint_zkbin.clone(),
            mint_pk: th.mint_pk.clone(),
            burn_zkbin: th.burn_zkbin.clone(),
            burn_pk: th.burn_pk.clone(),
            clear_input: false,
        }
        .build()?;
        let (params, proofs, secret_keys, spent_coins) = (
            call_debris.params,
            call_debris.proofs,
            call_debris.signature_secrets,
            call_debris.spent_coins,
        );

        info!(target: "money", "[Alice] ============================");
        info!(target: "money", "[Alice] Building payment tx to Alice");
        info!(target: "money", "[Alice] ============================");
        let mut data = vec![MoneyTransfer as u8];
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

        // Verify transaction
        info!(target: "money", "[Faucet] ================================");
        info!(target: "money", "[Faucet] Executing Alice2Alice payment tx");
        info!(target: "money", "[Faucet] ================================");
        th.faucet_state.read().await.verify_transactions(&[tx.clone()], true).await?;
        for output in &params.outputs {
            th.faucet_merkle_tree.append(&MerkleNode::from(output.coin.inner()));
        }
        info!(target: "money", "[Alice] ================================");
        info!(target: "money", "[Alice] Executing Alice2Alice payment tx");
        info!(target: "money", "[Alice] ================================");
        th.alice_state.read().await.verify_transactions(&[tx.clone()], true).await?;
        // Gather new owncoins and apply the state transitions
        for output in params.outputs {
            th.alice_merkle_tree.append(&MerkleNode::from(output.coin.inner()));
            let note: MoneyNote = output.note.decrypt(&th.alice_kp.secret)?;
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

        assert!(th.faucet_merkle_tree.root(0).unwrap() == th.alice_merkle_tree.root(0).unwrap());
    }

    Ok(())
}

/*
#[async_std::test]
async fn alice2alice_random_amounts_multiplecoins() -> Result<()> {
    init_logger()?;

    // N blocks to simulate
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

    // Initialize harness
    let mut th = MoneyTestHarness::new().await?;
    let contract_id = *MONEY_CONTRACT_ID;

    // Generating 10 airdrop coins
    let mut token_ids = vec![];
    let mut airdrops_amounts = vec![];
    let mut owncoins = vec![];
    for i in 0..10 {
        let token_id = TokenId::from(pallas::Base::random(&mut OsRng));
        let amount = rand::thread_rng().gen_range(1..1000);

        info!(target: "money", "Generating token {}: ID {} - amount {}", i, token_id, amount);

        let (airdrop_tx, airdrop_params) = th.airdrop(amount, token_id, &th.alice_kp.public)?;

        th.faucet_state.read().await.verify_transactions(&[airdrop_tx.clone()], true).await?;
        th.faucet_merkle_tree.append(&MerkleNode::from(airdrop_params.outputs[0].coin));

        th.alice_state.read().await.verify_transactions(&[airdrop_tx.clone()], true).await?;
        th.alice_merkle_tree.append(&MerkleNode::from(airdrop_params.outputs[0].coin));
        let leaf_position = th.alice_merkle_tree.witness().unwrap();

        let ciphertext = airdrop_params.outputs[0].ciphertext.clone();
        let ephem_public = airdrop_params.outputs[0].ephem_public;
        let e_note = EncryptedNote { ciphertext, ephem_public };
        let note = e_note.decrypt(&th.alice_kp.secret)?;

        owncoins.push(vec![OwnCoin {
            coin: Coin::from(airdrop_params.outputs[0].coin),
            note: note.clone(),
            secret: th.alice_kp.secret,
            nullifier: Nullifier::from(poseidon_hash([th.alice_kp.secret.inner(), note.serial])),
            leaf_position,
        }]);
        airdrops_amounts.push(amount);
        token_ids.push(token_id);
    }

    // Simulating N blocks
    for b in 0..n {
        info!(target: "money", "Generating transactions for block: {}", b);
        // Get a random sized sample of owncoins
        let sample =
            (0..10).choose_multiple(&mut rand::thread_rng(), rand::thread_rng().gen_range(1..10));
        info!(target: "money", "Coins to use: {:?}", sample);

        // Generate a transaction for each coin
        let mut txs = vec![];
        for index in sample {
            info!(target: "money", "Building Alice2Alice transfer tx for coin {}", index);

            let mut coins = owncoins[index].clone();
            let token_id = token_ids[index];
            let airdrop_amount = airdrops_amounts[index];

            info!(target: "money", "Alice coins: {}", coins.len());
            for (i, c) in coins.iter().enumerate() {
                info!(target: "money", "\t coin {} value: {}", i, c.note.value);
            }

            let amount = rand::thread_rng().gen_range(1..airdrop_amount);
            info!(target: "money", "Sending: {}", amount);

            let (params, proofs, secret_keys, spent_coins) = build_transfer_tx(
                &th.alice_kp,
                &th.alice_kp.public,
                amount,
                token_id,
                pallas::Base::zero(),
                pallas::Base::zero(),
                pallas::Base::random(&mut OsRng),
                &coins,
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
                coins.retain(|x| x != &spent);
            }

            // Store transaction
            txs.push(tx.clone());

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

                coins.push(owncoin);
            }

            // Replace coins
            owncoins[index] = coins;
        }

        // Apply the state transitions
        th.alice_state.read().await.verify_transactions(&txs, true).await?;
    }

    Ok(())
}
*/
