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

use darkfi::zk::{Witness, halo2::Field};
use darkfi_sdk::{
    crypto::{
        BaseBlind, FuncRef, MONEY_CONTRACT_ID, MerkleNode, MerkleTree, PublicKey, ScalarBlind,
        SecretKey,
        pasta_prelude::{Curve, CurveAffine},
        pedersen_commitment_u64,
        util::poseidon_hash,
    },
    pasta::{Fp, pallas, pallas::Base},
};
use halo2_proofs::circuit::Value;
use rand::rngs::OsRng;

pub fn mint_v1() -> (Vec<Witness>, Vec<Base>) {
    let secret_key = SecretKey::random(&mut OsRng);
    let pubkey = PublicKey::from_secret(secret_key);
    let coin_pub_x = pubkey.x();
    let coin_pub_y = pubkey.y();
    let coin_value = 23u64;
    let coin_token_id = Fp::random(&mut OsRng);
    let coin_spend_hook = Fp::from(0);
    let coin_user_data = Fp::from(0);
    let coin_blind = BaseBlind::random(&mut OsRng);
    let value_blind = ScalarBlind::random(&mut OsRng);
    let coin_token_id_blind = BaseBlind::random(&mut OsRng);

    let coin = poseidon_hash([
        coin_pub_x,
        coin_pub_y,
        Fp::from(coin_value),
        coin_token_id,
        coin_spend_hook,
        coin_user_data,
        coin_blind.inner(),
    ]);
    let value_commit = (pedersen_commitment_u64(coin_value, value_blind)).to_affine();
    let value_commit_x = *value_commit.coordinates().unwrap().x();
    let value_commit_y = *value_commit.coordinates().unwrap().y();
    let token_commit = poseidon_hash([coin_token_id, coin_token_id_blind.inner()]);

    let prover_witnesses = vec![
        Witness::Base(Value::known(coin_pub_x)),
        Witness::Base(Value::known(coin_pub_y)),
        Witness::Base(Value::known(pallas::Base::from(coin_value))),
        Witness::Base(Value::known(coin_token_id)),
        Witness::Base(Value::known(coin_spend_hook)),
        Witness::Base(Value::known(coin_user_data)),
        Witness::Base(Value::known(coin_blind.inner())),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(coin_token_id_blind.inner())),
    ];

    let public_inputs = vec![coin, value_commit_x, value_commit_y, token_commit];

    (prover_witnesses, public_inputs)
}

pub fn burn_v1() -> (Vec<Witness>, Vec<Base>) {
    let coin_secret = SecretKey::random(&mut OsRng);
    let coin_pubkey = PublicKey::from_secret(coin_secret);
    let coin_pub_x = coin_pubkey.x();
    let coin_pub_y = coin_pubkey.y();

    let sig_secret = SecretKey::random(&mut OsRng);
    let sig_pubkey = PublicKey::from_secret(sig_secret);
    let sig_pub_x = sig_pubkey.x();
    let sig_pub_y = sig_pubkey.y();

    let coin_value = 23u64;
    let coin_token_id = Fp::random(&mut OsRng);
    let coin_spend_hook = Fp::from(0);
    let coin_user_data = Fp::from(0);
    let coin_blind = BaseBlind::random(&mut OsRng);
    let value_blind = ScalarBlind::random(&mut OsRng);
    let coin_token_id_blind = BaseBlind::random(&mut OsRng);
    let user_data_blind = BaseBlind::random(&mut OsRng);

    let my_coin = poseidon_hash([
        coin_pub_x,
        coin_pub_y,
        Fp::from(coin_value),
        coin_token_id,
        coin_spend_hook,
        coin_user_data,
        coin_blind.inner(),
    ]);

    let mut tree = MerkleTree::new(u32::MAX as usize);
    let coin1 = MerkleNode::from(Fp::random(&mut OsRng));
    let coin2 = MerkleNode::from(Fp::random(&mut OsRng));
    tree.append(coin1);
    tree.mark();
    tree.append(MerkleNode::from(my_coin));
    let leaf_position = tree.mark().unwrap();
    tree.append(coin2);

    let root = tree.root(0).unwrap().inner();
    let merkle_path = tree.witness(leaf_position, 0).unwrap();

    let nullifier = poseidon_hash([coin_secret.inner(), my_coin]);
    let user_data_enc = poseidon_hash([coin_user_data, user_data_blind.inner()]);
    let value_commit = pedersen_commitment_u64(coin_value, value_blind).to_affine();
    let value_commit_x = *value_commit.coordinates().unwrap().x();
    let value_commit_y = *value_commit.coordinates().unwrap().y();
    let token_id_commit = poseidon_hash([coin_token_id, coin_token_id_blind.inner()]);

    let prover_witnesses = vec![
        Witness::Base(Value::known(coin_secret.inner())),
        Witness::Base(Value::known(Fp::from(coin_value))),
        Witness::Base(Value::known(coin_token_id)),
        Witness::Base(Value::known(coin_spend_hook)),
        Witness::Base(Value::known(coin_user_data)),
        Witness::Base(Value::known(coin_blind.inner())),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(coin_token_id_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(u64::from(leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(sig_secret.inner())),
    ];

    let public_inputs = vec![
        nullifier,
        value_commit_x,
        value_commit_y,
        token_id_commit,
        root,
        user_data_enc,
        coin_spend_hook,
        sig_pub_x,
        sig_pub_y,
    ];

    (prover_witnesses, public_inputs)
}

pub fn fee_v1() -> (Vec<Witness>, Vec<Base>) {
    let token_id = Fp::random(&mut OsRng);
    let token_id_blind = BaseBlind::random(&mut OsRng);
    let coin_secret = SecretKey::random(&mut OsRng);
    let coin_pubkey = PublicKey::from_secret(coin_secret);
    let coin_pub_x = coin_pubkey.x();
    let coin_pub_y = coin_pubkey.y();
    // Input Coin Info
    let input_coin_value = 23u64;
    let input_coin_spend_hook = Fp::from(0);
    let input_coin_user_data = Fp::from(0);
    let input_coin_blind = BaseBlind::random(&mut OsRng);
    let input_value_blind = ScalarBlind::random(&mut OsRng);
    let input_user_data_blind = BaseBlind::random(&mut OsRng);

    let input_coin = poseidon_hash([
        coin_pub_x,
        coin_pub_y,
        Fp::from(input_coin_value),
        token_id,
        input_coin_spend_hook,
        input_coin_user_data,
        input_coin_blind.inner(),
    ]);

    let input_coin_nullifier = poseidon_hash([coin_secret.inner(), input_coin]);
    let input_coin_user_data_enc =
        poseidon_hash([input_coin_user_data, input_user_data_blind.inner()]);
    let input_coin_value_commit =
        (pedersen_commitment_u64(input_coin_value, input_value_blind)).to_affine();
    let input_coin_value_commit_x = *input_coin_value_commit.coordinates().unwrap().x();
    let input_coin_value_commit_y = *input_coin_value_commit.coordinates().unwrap().y();

    let sig_secret = SecretKey::random(&mut OsRng);
    let sig_pubkey = PublicKey::from_secret(sig_secret);
    let sig_pub_x = sig_pubkey.x();
    let sig_pub_y = sig_pubkey.y();

    let mut tree = MerkleTree::new(u32::MAX as usize);
    let coin1 = MerkleNode::from(Fp::random(&mut OsRng));
    let coin2 = MerkleNode::from(Fp::random(&mut OsRng));
    tree.append(coin1);
    tree.mark();
    tree.append(MerkleNode::from(input_coin));
    let input_coin_leaf_position = tree.mark().unwrap();
    tree.append(coin2);

    let root = tree.root(0).unwrap().inner();
    let input_coin_merkle_path = tree.witness(input_coin_leaf_position, 0).unwrap();

    // Output Coin Info
    let output_coin_value = 22u64;
    let output_coin_spend_hook = Fp::from(0);
    let output_coin_user_data = Fp::from(0);
    let output_coin_blind = BaseBlind::random(&mut OsRng);
    let output_value_blind = ScalarBlind::random(&mut OsRng);

    let output_coin = poseidon_hash([
        coin_pub_x,
        coin_pub_y,
        Fp::from(output_coin_value),
        token_id,
        output_coin_spend_hook,
        output_coin_user_data,
        output_coin_blind.inner(),
    ]);

    let output_coin_value_commit =
        (pedersen_commitment_u64(output_coin_value, output_value_blind)).to_affine();
    let output_coin_value_commit_x = *output_coin_value_commit.coordinates().unwrap().x();
    let output_coin_value_commit_y = *output_coin_value_commit.coordinates().unwrap().y();
    let token_id_commit = poseidon_hash([token_id, token_id_blind.inner()]);

    let prover_witnesses = vec![
        Witness::Base(Value::known(coin_secret.inner())),
        Witness::Uint32(Value::known(u64::from(input_coin_leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input_coin_merkle_path.clone().try_into().unwrap())),
        Witness::Base(Value::known(sig_secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(input_coin_value))),
        Witness::Scalar(Value::known(input_value_blind.inner())),
        Witness::Base(Value::known(input_coin_spend_hook)),
        Witness::Base(Value::known(input_coin_user_data)),
        Witness::Base(Value::known(input_coin_blind.inner())),
        Witness::Base(Value::known(input_user_data_blind.inner())),
        Witness::Base(Value::known(pallas::Base::from(output_coin_value))),
        Witness::Base(Value::known(output_coin_spend_hook)),
        Witness::Base(Value::known(output_coin_user_data)),
        Witness::Scalar(Value::known(output_value_blind.inner())),
        Witness::Base(Value::known(output_coin_blind.inner())),
        Witness::Base(Value::known(token_id)),
        Witness::Base(Value::known(token_id_blind.inner())),
    ];

    let public_inputs = vec![
        input_coin_nullifier,
        input_coin_value_commit_x,
        input_coin_value_commit_y,
        token_id_commit,
        root,
        input_coin_user_data_enc,
        sig_pub_x,
        sig_pub_y,
        output_coin,
        output_coin_value_commit_x,
        output_coin_value_commit_y,
    ];

    (prover_witnesses, public_inputs)
}

pub fn token_mint_v1() -> (Vec<Witness>, Vec<Base>) {
    let secret_key = SecretKey::random(&mut OsRng);
    let pubkey = PublicKey::from_secret(secret_key);
    let coin_pub_x = pubkey.x();
    let coin_pub_y = pubkey.y();
    let coin_value = 23u64;
    let coin_spend_hook = Fp::from(0);
    let coin_user_data = Fp::from(0);
    let coin_blind = BaseBlind::random(&mut OsRng);

    // Token Attributes
    let token_auth_parent = FuncRef {
        contract_id: *MONEY_CONTRACT_ID,
        func_code: 0x05, //MoneyFunction::AuthTokenMintV1 as u8,
    }
    .to_func_id();
    let mint_authority = SecretKey::random(&mut OsRng);
    let (mint_auth_x, mint_auth_y) = PublicKey::from_secret(mint_authority).xy();
    let token_user_data = poseidon_hash([mint_auth_x, mint_auth_y]);
    let token_id_blind = BaseBlind::random(&mut OsRng);
    let token_id =
        poseidon_hash([token_auth_parent.inner(), token_user_data, token_id_blind.inner()]);

    let coin = poseidon_hash([
        coin_pub_x,
        coin_pub_y,
        Fp::from(coin_value),
        token_id,
        coin_spend_hook,
        coin_user_data,
        coin_blind.inner(),
    ]);

    let prover_witnesses = vec![
        // Coin attributes
        Witness::Base(Value::known(coin_pub_x)),
        Witness::Base(Value::known(coin_pub_y)),
        Witness::Base(Value::known(pallas::Base::from(coin_value))),
        Witness::Base(Value::known(coin_spend_hook)),
        Witness::Base(Value::known(coin_user_data)),
        Witness::Base(Value::known(coin_blind.inner())),
        // Token attributes
        Witness::Base(Value::known(token_auth_parent.inner())),
        Witness::Base(Value::known(token_user_data)),
        Witness::Base(Value::known(token_id_blind.inner())),
    ];

    let public_inputs = vec![token_auth_parent.inner(), coin];

    (prover_witnesses, public_inputs)
}

pub fn auth_token_mint_v1() -> (Vec<Witness>, Vec<Base>) {
    let token_auth_parent = FuncRef {
        contract_id: *MONEY_CONTRACT_ID,
        func_code: 0x05, //MoneyFunction::AuthTokenMintV1 as u8,
    }
    .to_func_id();
    let mint_authority = SecretKey::random(&mut OsRng);
    let (mint_pub_x, mint_pub_y) = PublicKey::from_secret(mint_authority).xy();
    let token_user_data = poseidon_hash([mint_pub_x, mint_pub_y]);
    let token_id_blind = BaseBlind::random(&mut OsRng);
    let token_id =
        poseidon_hash([token_auth_parent.inner(), token_user_data, token_id_blind.inner()]);

    let prover_witnesses = vec![
        // Token attributes
        Witness::Base(Value::known(token_auth_parent.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        // Secret key used by the mint authority
        Witness::Base(Value::known(mint_authority.inner())),
    ];

    let public_inputs = vec![mint_pub_x, mint_pub_y, token_id];

    (prover_witnesses, public_inputs)
}
