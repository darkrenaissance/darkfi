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
        BaseBlind, Blind, MerkleNode, MerkleTree, PublicKey, ScalarBlind, SecretKey,
        note::ElGamalEncryptedNote,
        pasta_prelude::{Curve, CurveAffine},
        pedersen_commitment_u64,
        smt::{EMPTY_NODES_FP, MemoryStorageFp, PoseidonFp, SmtMemoryFp},
        util::{fv_mod_fp_unsafe, poseidon_hash},
    },
    pasta::{Fp, Fq, pallas, pallas::Base},
};
use halo2_proofs::circuit::Value;
use rand::rngs::OsRng;

pub fn mint() -> (Vec<Witness>, Vec<Base>) {
    let proposer_limit = Fp::from(20);
    let quorum = Fp::from(10);
    let early_exec_quorum = Fp::from(10);
    let approval_ratio_quot = Fp::from(67);
    let approval_ratio_base = Fp::from(100);

    let dao_notes_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposer_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposals_secret_key = SecretKey::random(&mut OsRng);
    let dao_votes_secret_key = SecretKey::random(&mut OsRng);
    let dao_exec_secret_key = SecretKey::random(&mut OsRng);
    let dao_early_exec_secret_key = SecretKey::random(&mut OsRng);

    let dao_notes_pub_key = PublicKey::from_secret(dao_notes_secret_key);
    let dao_proposer_pub_key = PublicKey::from_secret(dao_proposer_secret_key);
    let dao_proposals_pub_key = PublicKey::from_secret(dao_proposals_secret_key);
    let dao_votes_pub_key = PublicKey::from_secret(dao_votes_secret_key);
    let dao_exec_pub_key = PublicKey::from_secret(dao_exec_secret_key);
    let dao_early_exec_pub_key = PublicKey::from_secret(dao_early_exec_secret_key);

    let gov_token_id = Fp::random(&mut OsRng);
    let bulla_blind = Fp::random(&mut OsRng);

    let bulla = poseidon_hash([
        proposer_limit,
        quorum,
        early_exec_quorum,
        approval_ratio_quot,
        approval_ratio_base,
        gov_token_id,
        dao_notes_pub_key.x(),
        dao_notes_pub_key.y(),
        dao_proposer_pub_key.x(),
        dao_proposer_pub_key.y(),
        dao_proposals_pub_key.x(),
        dao_proposals_pub_key.y(),
        dao_votes_pub_key.x(),
        dao_votes_pub_key.y(),
        dao_exec_pub_key.x(),
        dao_exec_pub_key.y(),
        dao_early_exec_pub_key.x(),
        dao_early_exec_pub_key.y(),
        bulla_blind,
    ]);

    let prover_witnesses = vec![
        Witness::Base(Value::known(proposer_limit)),
        Witness::Base(Value::known(quorum)),
        Witness::Base(Value::known(early_exec_quorum)),
        Witness::Base(Value::known(approval_ratio_quot)),
        Witness::Base(Value::known(approval_ratio_base)),
        Witness::Base(Value::known(gov_token_id)),
        Witness::Base(Value::known(dao_notes_secret_key.inner())),
        Witness::Base(Value::known(dao_proposer_secret_key.inner())),
        Witness::Base(Value::known(dao_proposals_secret_key.inner())),
        Witness::Base(Value::known(dao_votes_secret_key.inner())),
        Witness::Base(Value::known(dao_exec_secret_key.inner())),
        Witness::Base(Value::known(dao_early_exec_secret_key.inner())),
        Witness::Base(Value::known(bulla_blind)),
    ];

    let public_inputs = vec![dao_notes_pub_key.x(), dao_notes_pub_key.y(), bulla];

    (prover_witnesses, public_inputs)
}

pub fn propose_input() -> (Vec<Witness>, Vec<Base>) {
    // Governance coin setup
    let coin_secret = SecretKey::random(&mut OsRng);
    let coin_pubkey = PublicKey::from_secret(coin_secret);
    let coin_pub_x = coin_pubkey.x();
    let coin_pub_y = coin_pubkey.y();

    let sig_secret = SecretKey::random(&mut OsRng);
    let sig_pubkey = PublicKey::from_secret(sig_secret);
    let sig_pub_x = sig_pubkey.x();
    let sig_pub_y = sig_pubkey.y();

    let coin_value = 23u64;
    let gov_token_id = Fp::random(&mut OsRng);
    let coin_spend_hook = Fp::from(0);
    let coin_user_data = Fp::from(0);
    let coin_blind = BaseBlind::random(&mut OsRng);
    let value_blind = ScalarBlind::random(&mut OsRng);
    let gov_token_blind = BaseBlind::random(&mut OsRng);

    let my_coin = poseidon_hash([
        coin_pub_x,
        coin_pub_y,
        Fp::from(coin_value),
        gov_token_id,
        coin_spend_hook,
        coin_user_data,
        coin_blind.inner(),
    ]);
    let value_commit = (pedersen_commitment_u64(coin_value, value_blind)).to_affine();
    let value_commit_x = *value_commit.coordinates().unwrap().x();
    let value_commit_y = *value_commit.coordinates().unwrap().y();
    let gov_token_commit = poseidon_hash([gov_token_id, gov_token_blind.inner()]);

    // Merkle tree setup
    let mut tree = MerkleTree::new(u32::MAX as usize);
    let coin1 = MerkleNode::from(Fp::random(&mut OsRng));
    let coin2 = MerkleNode::from(Fp::random(&mut OsRng));
    tree.append(coin1);
    tree.mark();
    tree.append(MerkleNode::from(my_coin));
    let leaf_position = tree.mark().unwrap();
    tree.append(coin2);

    let merkle_root = tree.root(0).unwrap().inner();
    let merkle_path = tree.witness(leaf_position, 0).unwrap();
    // Sparse merkle tree setup
    let mut smt = SmtMemoryFp::new(MemoryStorageFp::new(), PoseidonFp::new(), &EMPTY_NODES_FP);

    let leaves = vec![Fp::random(&mut OsRng), Fp::random(&mut OsRng), Fp::random(&mut OsRng)];
    let leaves: Vec<_> = leaves.into_iter().map(|l| (l, l)).collect();
    smt.insert_batch(leaves.clone()).unwrap();

    let smt_null_root = smt.root();
    let nullifier = poseidon_hash([coin_secret.inner(), my_coin]);

    let smt_null_path = smt.prove_membership(&nullifier);
    if !smt_null_path.verify(&smt_null_root, &pallas::Base::ZERO, &nullifier) {
        panic!("smt null path verification_failed");
    }

    let prover_witnesses = vec![
        Witness::Base(Value::known(coin_secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(coin_value))),
        Witness::Base(Value::known(gov_token_id)),
        Witness::Base(Value::known(coin_spend_hook)),
        Witness::Base(Value::known(coin_user_data)),
        Witness::Base(Value::known(coin_blind.inner())),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(gov_token_blind.inner())),
        Witness::Uint32(Value::known(u64::from(leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(merkle_path.clone().try_into().unwrap())),
        Witness::SparseMerklePath(Value::known(smt_null_path.path)),
        Witness::Base(Value::known(sig_secret.inner())),
    ];

    let public_inputs = vec![
        smt_null_root,
        value_commit_x,
        value_commit_y,
        gov_token_commit,
        merkle_root,
        sig_pub_x,
        sig_pub_y,
    ];

    (prover_witnesses, public_inputs)
}

pub fn propose_main() -> (Vec<Witness>, Vec<Base>) {
    let proposer_limit = Fp::from(20);
    let quorum = Fp::from(10);
    let early_exec_quorum = Fp::from(10);
    let approval_ratio_quot = Fp::from(67);
    let approval_ratio_base = Fp::from(100);

    let dao_notes_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposer_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposals_secret_key = SecretKey::random(&mut OsRng);
    let dao_votes_secret_key = SecretKey::random(&mut OsRng);
    let dao_exec_secret_key = SecretKey::random(&mut OsRng);
    let dao_early_exec_secret_key = SecretKey::random(&mut OsRng);

    let dao_notes_pub_key = PublicKey::from_secret(dao_notes_secret_key);
    let dao_proposer_pub_key = PublicKey::from_secret(dao_proposer_secret_key);
    let dao_proposals_pub_key = PublicKey::from_secret(dao_proposals_secret_key);
    let dao_votes_pub_key = PublicKey::from_secret(dao_votes_secret_key);
    let dao_exec_pub_key = PublicKey::from_secret(dao_exec_secret_key);
    let dao_early_exec_pub_key = PublicKey::from_secret(dao_early_exec_secret_key);

    let gov_token_id = Fp::random(&mut OsRng);
    let gov_token_blind = BaseBlind::random(&mut OsRng);
    let gov_token_commit = poseidon_hash([gov_token_id, gov_token_blind.inner()]);
    let bulla_blind = Fp::random(&mut OsRng);

    let dao_bulla = poseidon_hash([
        proposer_limit,
        quorum,
        early_exec_quorum,
        approval_ratio_quot,
        approval_ratio_base,
        gov_token_id,
        dao_notes_pub_key.x(),
        dao_notes_pub_key.y(),
        dao_proposer_pub_key.x(),
        dao_proposer_pub_key.y(),
        dao_proposals_pub_key.x(),
        dao_proposals_pub_key.y(),
        dao_votes_pub_key.x(),
        dao_votes_pub_key.y(),
        dao_exec_pub_key.x(),
        dao_exec_pub_key.y(),
        dao_early_exec_pub_key.x(),
        dao_early_exec_pub_key.y(),
        bulla_blind,
    ]);

    // Store Dao Bulla in Merkle tree
    let mut tree = MerkleTree::new(u32::MAX as usize);
    let coin1 = MerkleNode::from(Fp::random(&mut OsRng));
    let coin2 = MerkleNode::from(Fp::random(&mut OsRng));
    tree.append(coin1);
    tree.mark();
    tree.append(MerkleNode::from(dao_bulla));
    let dao_leaf_position = tree.mark().unwrap();
    tree.append(coin2);

    let dao_merkle_root = tree.root(0).unwrap().inner();
    let dao_merkle_path = tree.witness(dao_leaf_position, 0).unwrap();

    // Proposal info
    let proposal_blind = BaseBlind::random(&mut OsRng);
    let proposal_auth_calls_commit = Fp::random(&mut OsRng);
    let proposal_creation_blockwindow = Fp::from(100000);
    let proposal_duration_blockwindows = Fp::from(20);
    let proposal_bulla = poseidon_hash([
        proposal_auth_calls_commit,
        proposal_creation_blockwindow,
        proposal_duration_blockwindows,
        Fp::ZERO,
        dao_bulla,
        proposal_blind.inner(),
    ]);

    let total_funds = 2300;
    let total_funds_blind = ScalarBlind::random(&mut OsRng);
    let total_funds_commit = pedersen_commitment_u64(total_funds, total_funds_blind);
    let total_funds_coords = total_funds_commit.to_affine().coordinates().unwrap();
    let total_funds = pallas::Base::from(total_funds);

    let prover_witnesses = vec![
        // Proposers total number of gov tokens
        Witness::Base(Value::known(total_funds)),
        Witness::Scalar(Value::known(total_funds_blind.inner())),
        // Used for blinding exported gov token ID
        Witness::Base(Value::known(gov_token_blind.inner())),
        // Proposal params
        Witness::Base(Value::known(proposal_auth_calls_commit)),
        Witness::Base(Value::known(proposal_creation_blockwindow)),
        Witness::Base(Value::known(proposal_duration_blockwindows)),
        Witness::Base(Value::known(Fp::ZERO)),
        Witness::Base(Value::known(proposal_blind.inner())),
        // DAO params
        Witness::Base(Value::known(proposer_limit)),
        Witness::Base(Value::known(quorum)),
        Witness::Base(Value::known(early_exec_quorum)),
        Witness::Base(Value::known(approval_ratio_quot)),
        Witness::Base(Value::known(approval_ratio_base)),
        Witness::Base(Value::known(gov_token_id)),
        Witness::Base(Value::known(dao_notes_pub_key.x())),
        Witness::Base(Value::known(dao_notes_pub_key.y())),
        Witness::Base(Value::known(dao_proposer_secret_key.inner())),
        Witness::Base(Value::known(dao_proposals_pub_key.x())),
        Witness::Base(Value::known(dao_proposals_pub_key.y())),
        Witness::Base(Value::known(dao_votes_pub_key.x())),
        Witness::Base(Value::known(dao_votes_pub_key.y())),
        Witness::Base(Value::known(dao_exec_pub_key.x())),
        Witness::Base(Value::known(dao_exec_pub_key.y())),
        Witness::Base(Value::known(dao_early_exec_pub_key.x())),
        Witness::Base(Value::known(dao_early_exec_pub_key.y())),
        Witness::Base(Value::known(bulla_blind)),
        Witness::Uint32(Value::known(u64::from(dao_leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(dao_merkle_path.try_into().unwrap())),
    ];

    let public_inputs = vec![
        gov_token_commit,
        dao_merkle_root,
        proposal_bulla,
        proposal_creation_blockwindow,
        *total_funds_coords.x(),
        *total_funds_coords.y(),
    ];

    (prover_witnesses, public_inputs)
}

pub fn vote_input() -> (Vec<Witness>, Vec<Base>) {
    // Governance coin setup
    let coin_secret = SecretKey::random(&mut OsRng);
    let coin_pubkey = PublicKey::from_secret(coin_secret);
    let coin_pub_x = coin_pubkey.x();
    let coin_pub_y = coin_pubkey.y();

    let sig_secret = SecretKey::random(&mut OsRng);
    let sig_pubkey = PublicKey::from_secret(sig_secret);
    let sig_pub_x = sig_pubkey.x();
    let sig_pub_y = sig_pubkey.y();

    let coin_value = 23u64;
    let gov_token_id = Fp::random(&mut OsRng);
    let coin_spend_hook = Fp::from(0);
    let coin_user_data = Fp::from(0);
    let coin_blind = BaseBlind::random(&mut OsRng);
    let value_blind = ScalarBlind::random(&mut OsRng);
    let gov_token_blind = BaseBlind::random(&mut OsRng);

    let my_coin = poseidon_hash([
        coin_pub_x,
        coin_pub_y,
        Fp::from(coin_value),
        gov_token_id,
        coin_spend_hook,
        coin_user_data,
        coin_blind.inner(),
    ]);
    let value_commit = (pedersen_commitment_u64(coin_value, value_blind)).to_affine();
    let value_commit_x = *value_commit.coordinates().unwrap().x();
    let value_commit_y = *value_commit.coordinates().unwrap().y();
    let gov_token_commit = poseidon_hash([gov_token_id, gov_token_blind.inner()]);

    // Merkle tree setup
    let mut tree = MerkleTree::new(u32::MAX as usize);
    let coin1 = MerkleNode::from(Fp::random(&mut OsRng));
    let coin2 = MerkleNode::from(Fp::random(&mut OsRng));
    tree.append(coin1);
    tree.mark();
    tree.append(MerkleNode::from(my_coin));
    let leaf_position = tree.mark().unwrap();
    tree.append(coin2);

    let merkle_root = tree.root(0).unwrap().inner();
    let merkle_path = tree.witness(leaf_position, 0).unwrap();
    // Sparse merkle tree setup
    let mut smt = SmtMemoryFp::new(MemoryStorageFp::new(), PoseidonFp::new(), &EMPTY_NODES_FP);

    let leaves = vec![Fp::random(&mut OsRng), Fp::random(&mut OsRng), Fp::random(&mut OsRng)];
    let leaves: Vec<_> = leaves.into_iter().map(|l| (l, l)).collect();
    smt.insert_batch(leaves.clone()).unwrap();

    let smt_null_root = smt.root();
    let nullifier = poseidon_hash([coin_secret.inner(), my_coin]);

    let smt_null_path = smt.prove_membership(&nullifier);
    if !smt_null_path.verify(&smt_null_root, &pallas::Base::ZERO, &nullifier) {
        panic!("smt null path verification_failed");
    }

    let proposal_bulla = Fp::random(&mut OsRng);
    let vote_nullifier = poseidon_hash([nullifier, coin_secret.inner(), proposal_bulla]);

    let prover_witnesses = vec![
        Witness::Base(Value::known(coin_secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(coin_value))),
        Witness::Base(Value::known(gov_token_id)),
        Witness::Base(Value::known(coin_spend_hook)),
        Witness::Base(Value::known(coin_user_data)),
        Witness::Base(Value::known(coin_blind.inner())),
        Witness::Base(Value::known(proposal_bulla)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(gov_token_blind.inner())),
        Witness::Uint32(Value::known(u64::from(leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(merkle_path.clone().try_into().unwrap())),
        Witness::SparseMerklePath(Value::known(smt_null_path.path)),
        Witness::Base(Value::known(sig_secret.inner())),
    ];

    let public_inputs = vec![
        smt_null_root,
        proposal_bulla,
        vote_nullifier,
        value_commit_x,
        value_commit_y,
        gov_token_commit,
        merkle_root,
        sig_pub_x,
        sig_pub_y,
    ];

    (prover_witnesses, public_inputs)
}

pub fn vote_main() -> (Vec<Witness>, Vec<Base>) {
    let proposer_limit = Fp::from(20);
    let quorum = Fp::from(10);
    let early_exec_quorum = Fp::from(10);
    let approval_ratio_quot = Fp::from(67);
    let approval_ratio_base = Fp::from(100);

    let dao_notes_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposer_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposals_secret_key = SecretKey::random(&mut OsRng);
    let dao_votes_secret_key = SecretKey::random(&mut OsRng);
    let dao_exec_secret_key = SecretKey::random(&mut OsRng);
    let dao_early_exec_secret_key = SecretKey::random(&mut OsRng);

    let dao_notes_pub_key = PublicKey::from_secret(dao_notes_secret_key);
    let dao_proposer_pub_key = PublicKey::from_secret(dao_proposer_secret_key);
    let dao_proposals_pub_key = PublicKey::from_secret(dao_proposals_secret_key);
    let dao_votes_pub_key = PublicKey::from_secret(dao_votes_secret_key);
    let dao_exec_pub_key = PublicKey::from_secret(dao_exec_secret_key);
    let dao_early_exec_pub_key = PublicKey::from_secret(dao_early_exec_secret_key);

    let gov_token_id = Fp::random(&mut OsRng);
    let gov_token_blind = BaseBlind::random(&mut OsRng);
    let gov_token_commit = poseidon_hash([gov_token_id, gov_token_blind.inner()]);
    let bulla_blind = Fp::random(&mut OsRng);

    let dao_bulla = poseidon_hash([
        proposer_limit,
        quorum,
        early_exec_quorum,
        approval_ratio_quot,
        approval_ratio_base,
        gov_token_id,
        dao_notes_pub_key.x(),
        dao_notes_pub_key.y(),
        dao_proposer_pub_key.x(),
        dao_proposer_pub_key.y(),
        dao_proposals_pub_key.x(),
        dao_proposals_pub_key.y(),
        dao_votes_pub_key.x(),
        dao_votes_pub_key.y(),
        dao_exec_pub_key.x(),
        dao_exec_pub_key.y(),
        dao_early_exec_pub_key.x(),
        dao_early_exec_pub_key.y(),
        bulla_blind,
    ]);

    // Proposal info
    let proposal_blind = BaseBlind::random(&mut OsRng);
    let proposal_auth_calls_commit = Fp::random(&mut OsRng);
    let proposal_creation_blockwindow = Fp::from(100000);
    let proposal_duration_blockwindows = Fp::from(20);
    let proposal_bulla = poseidon_hash([
        proposal_auth_calls_commit,
        proposal_creation_blockwindow,
        proposal_duration_blockwindows,
        Fp::ZERO,
        dao_bulla,
        proposal_blind.inner(),
    ]);

    // Vote Info
    let vote_option = 1u64;
    let yes_vote_blind = loop {
        let blind = pallas::Scalar::random(&mut OsRng);
        if fv_mod_fp_unsafe(blind).is_some().into() {
            break blind
        }
    };
    let all_vote_blind = Fq::random(&mut OsRng);
    let all_vote_value = 10000;

    let yes_vote_commit =
        pedersen_commitment_u64(vote_option * all_vote_value, Blind(yes_vote_blind));
    let yes_vote_commit_coords = yes_vote_commit.to_affine().coordinates().unwrap();
    let all_vote_commit = pedersen_commitment_u64(all_vote_value, Blind(all_vote_blind));
    let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

    let yes_vote_blind = Blind(fv_mod_fp_unsafe(yes_vote_blind).unwrap());
    let all_vote_blind = Blind(fv_mod_fp_unsafe(all_vote_blind).unwrap());

    let vote_option = Fp::from(vote_option);
    let all_vote_value_fp = Fp::from(all_vote_value);
    let ephem_secret = SecretKey::random(&mut OsRng);
    let ephem_pubkey = PublicKey::from_secret(ephem_secret);
    let (ephem_x, ephem_y) = ephem_pubkey.xy();
    let current_blockwindow = Fp::from(100005);

    let note = [vote_option, yes_vote_blind.inner(), all_vote_value_fp, all_vote_blind.inner()];
    let enc_note =
        ElGamalEncryptedNote::encrypt_unsafe(note, &ephem_secret, &dao_votes_pub_key).unwrap();

    let prover_witnesses = vec![
        // Proposal params
        Witness::Base(Value::known(proposal_auth_calls_commit)),
        Witness::Base(Value::known(proposal_creation_blockwindow)),
        Witness::Base(Value::known(proposal_duration_blockwindows)),
        Witness::Base(Value::known(Fp::ZERO)),
        Witness::Base(Value::known(proposal_blind.inner())),
        // DAO params
        Witness::Base(Value::known(proposer_limit)),
        Witness::Base(Value::known(quorum)),
        Witness::Base(Value::known(early_exec_quorum)),
        Witness::Base(Value::known(approval_ratio_quot)),
        Witness::Base(Value::known(approval_ratio_base)),
        Witness::Base(Value::known(gov_token_id)),
        Witness::Base(Value::known(dao_notes_pub_key.x())),
        Witness::Base(Value::known(dao_notes_pub_key.y())),
        Witness::Base(Value::known(dao_proposer_pub_key.x())),
        Witness::Base(Value::known(dao_proposer_pub_key.y())),
        Witness::Base(Value::known(dao_proposals_pub_key.x())),
        Witness::Base(Value::known(dao_proposals_pub_key.y())),
        Witness::EcNiPoint(Value::known(dao_votes_pub_key.inner())),
        Witness::Base(Value::known(dao_exec_pub_key.x())),
        Witness::Base(Value::known(dao_exec_pub_key.y())),
        Witness::Base(Value::known(dao_early_exec_pub_key.x())),
        Witness::Base(Value::known(dao_early_exec_pub_key.y())),
        Witness::Base(Value::known(bulla_blind)),
        // Vote
        Witness::Base(Value::known(vote_option)),
        Witness::Base(Value::known(yes_vote_blind.inner())),
        // Total number of gov tokens allocated
        Witness::Base(Value::known(all_vote_value_fp)),
        Witness::Base(Value::known(all_vote_blind.inner())),
        // Gov token
        Witness::Base(Value::known(gov_token_blind.inner())),
        // Time checks
        Witness::Base(Value::known(current_blockwindow)),
        // verifiable encryption
        Witness::Base(Value::known(ephem_secret.inner())),
    ];

    let public_inputs = vec![
        gov_token_commit,
        proposal_bulla,
        *yes_vote_commit_coords.x(),
        *yes_vote_commit_coords.y(),
        *all_vote_commit_coords.x(),
        *all_vote_commit_coords.y(),
        current_blockwindow,
        ephem_x,
        ephem_y,
        enc_note.encrypted_values[0],
        enc_note.encrypted_values[1],
        enc_note.encrypted_values[2],
        enc_note.encrypted_values[3],
    ];

    (prover_witnesses, public_inputs)
}

pub fn exec() -> (Vec<Witness>, Vec<Base>) {
    let proposer_limit = Fp::from(20);
    let quorum = Fp::from(10);
    let early_exec_quorum = Fp::from(10);
    let approval_ratio_quot = Fp::from(67);
    let approval_ratio_base = Fp::from(100);

    let dao_notes_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposer_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposals_secret_key = SecretKey::random(&mut OsRng);
    let dao_votes_secret_key = SecretKey::random(&mut OsRng);
    let dao_exec_secret_key = SecretKey::random(&mut OsRng);
    let dao_early_exec_secret_key = SecretKey::random(&mut OsRng);

    let dao_notes_pub_key = PublicKey::from_secret(dao_notes_secret_key);
    let dao_proposer_pub_key = PublicKey::from_secret(dao_proposer_secret_key);
    let dao_proposals_pub_key = PublicKey::from_secret(dao_proposals_secret_key);
    let dao_votes_pub_key = PublicKey::from_secret(dao_votes_secret_key);
    let dao_exec_pub_key = PublicKey::from_secret(dao_exec_secret_key);
    let dao_early_exec_pub_key = PublicKey::from_secret(dao_early_exec_secret_key);

    let gov_token_id = Fp::random(&mut OsRng);
    let bulla_blind = Fp::random(&mut OsRng);

    let dao_bulla = poseidon_hash([
        proposer_limit,
        quorum,
        early_exec_quorum,
        approval_ratio_quot,
        approval_ratio_base,
        gov_token_id,
        dao_notes_pub_key.x(),
        dao_notes_pub_key.y(),
        dao_proposer_pub_key.x(),
        dao_proposer_pub_key.y(),
        dao_proposals_pub_key.x(),
        dao_proposals_pub_key.y(),
        dao_votes_pub_key.x(),
        dao_votes_pub_key.y(),
        dao_exec_pub_key.x(),
        dao_exec_pub_key.y(),
        dao_early_exec_pub_key.x(),
        dao_early_exec_pub_key.y(),
        bulla_blind,
    ]);

    // Proposal info
    let proposal_blind = BaseBlind::random(&mut OsRng);
    let proposal_auth_calls_commit = Fp::random(&mut OsRng);
    let proposal_creation_blockwindow = Fp::from(100000);
    let proposal_duration_blockwindows = Fp::from(20);
    let proposal_bulla = poseidon_hash([
        proposal_auth_calls_commit,
        proposal_creation_blockwindow,
        proposal_duration_blockwindows,
        Fp::ZERO,
        dao_bulla,
        proposal_blind.inner(),
    ]);

    // Vote Info
    let yes_vote_value = 10000u64;
    let yes_vote_blind = ScalarBlind::random(&mut OsRng);
    let yes_vote_commit = pedersen_commitment_u64(yes_vote_value, yes_vote_blind);
    let yes_vote_commit_coords = yes_vote_commit.to_affine().coordinates().unwrap();

    let all_vote_value = 12000u64;
    let all_vote_blind = ScalarBlind::random(&mut OsRng);
    let all_vote_commit = pedersen_commitment_u64(all_vote_value, all_vote_blind);
    let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

    let current_blockwindow = Fp::from(100075);

    let sig_secret = SecretKey::random(&mut OsRng);
    let sig_pubkey = PublicKey::from_secret(sig_secret);
    let sig_pub_x = sig_pubkey.x();
    let sig_pub_y = sig_pubkey.y();

    let prover_witnesses = vec![
        // Proposal params
        Witness::Base(Value::known(proposal_auth_calls_commit)),
        Witness::Base(Value::known(pallas::Base::from(proposal_creation_blockwindow))),
        Witness::Base(Value::known(pallas::Base::from(proposal_duration_blockwindows))),
        Witness::Base(Value::known(Fp::ZERO)),
        Witness::Base(Value::known(proposal_blind.inner())),
        // DAO params
        Witness::Base(Value::known(proposer_limit)),
        Witness::Base(Value::known(quorum)),
        Witness::Base(Value::known(early_exec_quorum)),
        Witness::Base(Value::known(approval_ratio_quot)),
        Witness::Base(Value::known(approval_ratio_base)),
        Witness::Base(Value::known(gov_token_id)),
        Witness::Base(Value::known(dao_notes_pub_key.x())),
        Witness::Base(Value::known(dao_notes_pub_key.y())),
        Witness::Base(Value::known(dao_proposer_pub_key.x())),
        Witness::Base(Value::known(dao_proposer_pub_key.y())),
        Witness::Base(Value::known(dao_proposals_pub_key.x())),
        Witness::Base(Value::known(dao_proposals_pub_key.y())),
        Witness::Base(Value::known(dao_votes_pub_key.x())),
        Witness::Base(Value::known(dao_votes_pub_key.y())),
        Witness::Base(Value::known(dao_exec_secret_key.inner())),
        Witness::Base(Value::known(dao_early_exec_pub_key.x())),
        Witness::Base(Value::known(dao_early_exec_pub_key.y())),
        Witness::Base(Value::known(bulla_blind)),
        // Votes
        Witness::Base(Value::known(Fp::from(yes_vote_value))),
        Witness::Base(Value::known(Fp::from(all_vote_value))),
        Witness::Scalar(Value::known(yes_vote_blind.inner())),
        Witness::Scalar(Value::known(all_vote_blind.inner())),
        // Time checks
        Witness::Base(Value::known(current_blockwindow)),
        // Signature secret
        Witness::Base(Value::known(sig_secret.inner())),
    ];

    let public_inputs = vec![
        proposal_bulla,
        proposal_auth_calls_commit,
        current_blockwindow,
        *yes_vote_commit_coords.x(),
        *yes_vote_commit_coords.y(),
        *all_vote_commit_coords.x(),
        *all_vote_commit_coords.y(),
        sig_pub_x,
        sig_pub_y,
    ];

    (prover_witnesses, public_inputs)
}

pub fn early_exec() -> (Vec<Witness>, Vec<Base>) {
    let proposer_limit = Fp::from(20);
    let quorum = Fp::from(10);
    let early_exec_quorum = Fp::from(10);
    let approval_ratio_quot = Fp::from(67);
    let approval_ratio_base = Fp::from(100);

    let dao_notes_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposer_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposals_secret_key = SecretKey::random(&mut OsRng);
    let dao_votes_secret_key = SecretKey::random(&mut OsRng);
    let dao_exec_secret_key = SecretKey::random(&mut OsRng);
    let dao_early_exec_secret_key = SecretKey::random(&mut OsRng);

    let dao_notes_pub_key = PublicKey::from_secret(dao_notes_secret_key);
    let dao_proposer_pub_key = PublicKey::from_secret(dao_proposer_secret_key);
    let dao_proposals_pub_key = PublicKey::from_secret(dao_proposals_secret_key);
    let dao_votes_pub_key = PublicKey::from_secret(dao_votes_secret_key);
    let dao_exec_pub_key = PublicKey::from_secret(dao_exec_secret_key);
    let dao_early_exec_pub_key = PublicKey::from_secret(dao_early_exec_secret_key);

    let gov_token_id = Fp::random(&mut OsRng);
    let bulla_blind = Fp::random(&mut OsRng);

    let dao_bulla = poseidon_hash([
        proposer_limit,
        quorum,
        early_exec_quorum,
        approval_ratio_quot,
        approval_ratio_base,
        gov_token_id,
        dao_notes_pub_key.x(),
        dao_notes_pub_key.y(),
        dao_proposer_pub_key.x(),
        dao_proposer_pub_key.y(),
        dao_proposals_pub_key.x(),
        dao_proposals_pub_key.y(),
        dao_votes_pub_key.x(),
        dao_votes_pub_key.y(),
        dao_exec_pub_key.x(),
        dao_exec_pub_key.y(),
        dao_early_exec_pub_key.x(),
        dao_early_exec_pub_key.y(),
        bulla_blind,
    ]);

    // Proposal info
    let proposal_blind = BaseBlind::random(&mut OsRng);
    let proposal_auth_calls_commit = Fp::random(&mut OsRng);
    let proposal_creation_blockwindow = Fp::from(100000);
    let proposal_duration_blockwindows = Fp::from(20);
    let proposal_bulla = poseidon_hash([
        proposal_auth_calls_commit,
        proposal_creation_blockwindow,
        proposal_duration_blockwindows,
        Fp::ZERO,
        dao_bulla,
        proposal_blind.inner(),
    ]);

    // Vote Info
    let yes_vote_value = 10000u64;
    let yes_vote_blind = ScalarBlind::random(&mut OsRng);
    let yes_vote_commit = pedersen_commitment_u64(yes_vote_value, yes_vote_blind);
    let yes_vote_commit_coords = yes_vote_commit.to_affine().coordinates().unwrap();

    let all_vote_value = 10000u64;
    let all_vote_blind = ScalarBlind::random(&mut OsRng);
    let all_vote_commit = pedersen_commitment_u64(all_vote_value, all_vote_blind);
    let all_vote_commit_coords = all_vote_commit.to_affine().coordinates().unwrap();

    let current_blockwindow = Fp::from(100015);

    let sig_secret = SecretKey::random(&mut OsRng);
    let sig_pubkey = PublicKey::from_secret(sig_secret);
    let sig_pub_x = sig_pubkey.x();
    let sig_pub_y = sig_pubkey.y();

    let prover_witnesses = vec![
        // Proposal params
        Witness::Base(Value::known(proposal_auth_calls_commit)),
        Witness::Base(Value::known(pallas::Base::from(proposal_creation_blockwindow))),
        Witness::Base(Value::known(pallas::Base::from(proposal_duration_blockwindows))),
        Witness::Base(Value::known(Fp::ZERO)),
        Witness::Base(Value::known(proposal_blind.inner())),
        // DAO params
        Witness::Base(Value::known(proposer_limit)),
        Witness::Base(Value::known(quorum)),
        Witness::Base(Value::known(early_exec_quorum)),
        Witness::Base(Value::known(approval_ratio_quot)),
        Witness::Base(Value::known(approval_ratio_base)),
        Witness::Base(Value::known(gov_token_id)),
        Witness::Base(Value::known(dao_notes_pub_key.x())),
        Witness::Base(Value::known(dao_notes_pub_key.y())),
        Witness::Base(Value::known(dao_proposer_pub_key.x())),
        Witness::Base(Value::known(dao_proposer_pub_key.y())),
        Witness::Base(Value::known(dao_proposals_pub_key.x())),
        Witness::Base(Value::known(dao_proposals_pub_key.y())),
        Witness::Base(Value::known(dao_votes_pub_key.x())),
        Witness::Base(Value::known(dao_votes_pub_key.y())),
        Witness::Base(Value::known(dao_exec_secret_key.inner())),
        Witness::Base(Value::known(dao_early_exec_secret_key.inner())),
        Witness::Base(Value::known(bulla_blind)),
        // Votes
        Witness::Base(Value::known(Fp::from(yes_vote_value))),
        Witness::Base(Value::known(Fp::from(all_vote_value))),
        Witness::Scalar(Value::known(yes_vote_blind.inner())),
        Witness::Scalar(Value::known(all_vote_blind.inner())),
        // Time checks
        Witness::Base(Value::known(current_blockwindow)),
        // Signature secret
        Witness::Base(Value::known(sig_secret.inner())),
    ];

    let public_inputs = vec![
        proposal_bulla,
        proposal_auth_calls_commit,
        current_blockwindow,
        *yes_vote_commit_coords.x(),
        *yes_vote_commit_coords.y(),
        *all_vote_commit_coords.x(),
        *all_vote_commit_coords.y(),
        sig_pub_x,
        sig_pub_y,
    ];

    (prover_witnesses, public_inputs)
}

pub fn auth_money_transfer() -> (Vec<Witness>, Vec<Base>) {
    let proposer_limit = Fp::from(20);
    let quorum = Fp::from(10);
    let early_exec_quorum = Fp::from(10);
    let approval_ratio_quot = Fp::from(67);
    let approval_ratio_base = Fp::from(100);

    let dao_notes_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposer_secret_key = SecretKey::random(&mut OsRng);
    let dao_proposals_secret_key = SecretKey::random(&mut OsRng);
    let dao_votes_secret_key = SecretKey::random(&mut OsRng);
    let dao_exec_secret_key = SecretKey::random(&mut OsRng);
    let dao_early_exec_secret_key = SecretKey::random(&mut OsRng);

    let dao_notes_pub_key = PublicKey::from_secret(dao_notes_secret_key);
    let dao_proposer_pub_key = PublicKey::from_secret(dao_proposer_secret_key);
    let dao_proposals_pub_key = PublicKey::from_secret(dao_proposals_secret_key);
    let dao_votes_pub_key = PublicKey::from_secret(dao_votes_secret_key);
    let dao_exec_pub_key = PublicKey::from_secret(dao_exec_secret_key);
    let dao_early_exec_pub_key = PublicKey::from_secret(dao_early_exec_secret_key);

    let gov_token_id = Fp::random(&mut OsRng);
    let bulla_blind = Fp::random(&mut OsRng);

    let dao_bulla = poseidon_hash([
        proposer_limit,
        quorum,
        early_exec_quorum,
        approval_ratio_quot,
        approval_ratio_base,
        gov_token_id,
        dao_notes_pub_key.x(),
        dao_notes_pub_key.y(),
        dao_proposer_pub_key.x(),
        dao_proposer_pub_key.y(),
        dao_proposals_pub_key.x(),
        dao_proposals_pub_key.y(),
        dao_votes_pub_key.x(),
        dao_votes_pub_key.y(),
        dao_exec_pub_key.x(),
        dao_exec_pub_key.y(),
        dao_early_exec_pub_key.x(),
        dao_early_exec_pub_key.y(),
        bulla_blind,
    ]);

    // Proposal info
    let proposal_blind = BaseBlind::random(&mut OsRng);
    let proposal_auth_calls_commit = Fp::random(&mut OsRng);
    let proposal_creation_blockwindow = Fp::from(100000);
    let proposal_duration_blockwindows = Fp::from(20);
    let proposal_bulla = poseidon_hash([
        proposal_auth_calls_commit,
        proposal_creation_blockwindow,
        proposal_duration_blockwindows,
        Fp::ZERO,
        dao_bulla,
        proposal_blind.inner(),
    ]);

    // Coin Info
    let coin_token_id = Fp::random(&mut OsRng);
    let coin_blind = BaseBlind::random(&mut OsRng);
    let dao_spend_hook = Fp::from(3); //Dao::Exec

    let dao_change_value = pallas::Base::from(23u64);
    let dao_change_coin = poseidon_hash([
        dao_notes_pub_key.x(),
        dao_notes_pub_key.y(),
        dao_change_value,
        coin_token_id,
        dao_spend_hook,
        dao_bulla,
        coin_blind.inner(),
    ]);

    let ephem_secret = SecretKey::random(&mut OsRng);
    let change_ephem_pubkey = PublicKey::from_secret(ephem_secret);
    let (ephem_x, ephem_y) = change_ephem_pubkey.xy();

    let note = [dao_change_value, coin_token_id, coin_blind.inner()];

    let dao_change_attrs =
        ElGamalEncryptedNote::encrypt_unsafe(note, &ephem_secret, &dao_notes_pub_key).unwrap();

    let input_user_data_blind = BaseBlind::random(&mut OsRng);
    let input_user_data_enc = poseidon_hash([dao_bulla, input_user_data_blind.inner()]);

    let prover_witnesses = vec![
        // proposal params
        Witness::Base(Value::known(proposal_auth_calls_commit)),
        Witness::Base(Value::known(proposal_creation_blockwindow)),
        Witness::Base(Value::known(pallas::Base::from(proposal_duration_blockwindows))),
        Witness::Base(Value::known(Fp::ZERO)),
        Witness::Base(Value::known(proposal_blind.inner())),
        // DAO params
        Witness::Base(Value::known(proposer_limit)),
        Witness::Base(Value::known(quorum)),
        Witness::Base(Value::known(early_exec_quorum)),
        Witness::Base(Value::known(approval_ratio_quot)),
        Witness::Base(Value::known(approval_ratio_base)),
        Witness::Base(Value::known(gov_token_id)),
        Witness::EcNiPoint(Value::known(dao_notes_pub_key.inner())),
        Witness::Base(Value::known(dao_proposer_pub_key.x())),
        Witness::Base(Value::known(dao_proposer_pub_key.y())),
        Witness::Base(Value::known(dao_proposals_pub_key.x())),
        Witness::Base(Value::known(dao_proposals_pub_key.y())),
        Witness::Base(Value::known(dao_votes_pub_key.x())),
        Witness::Base(Value::known(dao_votes_pub_key.y())),
        Witness::Base(Value::known(dao_exec_pub_key.x())),
        Witness::Base(Value::known(dao_exec_pub_key.y())),
        Witness::Base(Value::known(dao_early_exec_pub_key.x())),
        Witness::Base(Value::known(dao_early_exec_pub_key.y())),
        Witness::Base(Value::known(bulla_blind)),
        // Dao input user data blind
        Witness::Base(Value::known(input_user_data_blind.inner())),
        // Dao output coin attrs
        Witness::Base(Value::known(dao_change_value)),
        Witness::Base(Value::known(coin_token_id)),
        Witness::Base(Value::known(coin_blind.inner())),
        // DAO::exec() func ID
        Witness::Base(Value::known(dao_spend_hook)),
        // Encrypted change DAO output
        Witness::Base(Value::known(ephem_secret.inner())),
    ];

    let public_inputs = vec![
        proposal_bulla,
        input_user_data_enc,
        dao_change_coin,
        dao_spend_hook,
        proposal_auth_calls_commit,
        ephem_x,
        ephem_y,
        dao_change_attrs.encrypted_values[0],
        dao_change_attrs.encrypted_values[1],
        dao_change_attrs.encrypted_values[2],
    ];

    (prover_witnesses, public_inputs)
}

pub fn auth_money_transfer_enc_coin() -> (Vec<Witness>, Vec<Base>) {
    // Coin Info
    let coin_secret = SecretKey::random(&mut OsRng);
    let coin_pub_key = PublicKey::from_secret(coin_secret);
    let coin_token_id = Fp::random(&mut OsRng);
    let coin_blind = BaseBlind::random(&mut OsRng);
    let coin_spend_hook = Fp::ZERO;
    let coin_user_data = Fp::ZERO;

    let coin_value = pallas::Base::from(23u64);
    let coin = poseidon_hash([
        coin_pub_key.x(),
        coin_pub_key.y(),
        coin_value,
        coin_token_id,
        coin_spend_hook,
        coin_user_data,
        coin_blind.inner(),
    ]);

    let ephem_secret = SecretKey::random(&mut OsRng);
    let change_ephem_pubkey = PublicKey::from_secret(ephem_secret);
    let (ephem_x, ephem_y) = change_ephem_pubkey.xy();

    let note = [coin_value, coin_token_id, coin_spend_hook, coin_user_data, coin_blind.inner()];
    let enc_note =
        ElGamalEncryptedNote::encrypt_unsafe(note, &ephem_secret, &coin_pub_key).unwrap();

    let prover_witnesses = vec![
        Witness::EcNiPoint(Value::known(coin_pub_key.inner())),
        Witness::Base(Value::known(coin_value)),
        Witness::Base(Value::known(coin_token_id)),
        Witness::Base(Value::known(coin_spend_hook)),
        Witness::Base(Value::known(coin_user_data)),
        Witness::Base(Value::known(coin_blind.inner())),
        Witness::Base(Value::known(ephem_secret.inner())),
    ];

    let public_inputs = vec![
        coin,
        ephem_x,
        ephem_y,
        enc_note.encrypted_values[0],
        enc_note.encrypted_values[1],
        enc_note.encrypted_values[2],
        enc_note.encrypted_values[3],
        enc_note.encrypted_values[4],
    ];

    (prover_witnesses, public_inputs)
}
