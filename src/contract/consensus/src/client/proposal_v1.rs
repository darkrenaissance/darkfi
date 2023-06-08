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

//! This API is crufty. Please rework it into something nice to read and nice to use.

use darkfi::{
    consensus::SlotCheckpoint,
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_money_contract::{
    client::{ConsensusNote, ConsensusOwnCoin},
    model::{Coin, ConsensusInput, ConsensusOutput},
};
use darkfi_sdk::{
    crypto::{
        ecvrf::VrfProof, note::AeadEncryptedNote, pasta_prelude::*, pedersen_commitment_base,
        pedersen_commitment_u64, poseidon_hash, MerkleNode, MerkleTree, Nullifier, PublicKey,
        SecretKey,
    },
    incrementalmerkletree::{Hashable, Tree},
    pasta::{group::ff::FromUniformBytes, pallas},
};
use log::debug;
use rand::rngs::OsRng;

use crate::{
    client::common::{ConsensusBurnInputInfo, ConsensusMintOutputInfo},
    model::{
        ConsensusProposalParamsV1, HEADSTART, MU_RHO_PREFIX, MU_Y_PREFIX, REWARD, REWARD_PALLAS,
        SEED_PREFIX, SERIAL_PREFIX,
    },
};

pub struct ConsensusProposalCallDebris {
    pub params: ConsensusProposalParamsV1,
    pub proofs: Vec<Proof>,
    pub signature_secret: SecretKey,
}

pub struct ConsensusProposalRevealed {
    pub nullifier: Nullifier,
    pub epoch: u64,
    pub public_key: PublicKey,
    pub merkle_root: MerkleNode,
    pub value_commit: pallas::Point,
    pub new_serial: pallas::Base,
    pub new_serial_commit: pallas::Point,
    pub new_value_commit: pallas::Point,
    pub new_coin: Coin,
    pub vrf_proof: VrfProof,
    pub mu_y: pallas::Base,
    pub y: pallas::Base,
    pub mu_rho: pallas::Base,
    pub rho: pallas::Base,
    pub sigma1: pallas::Base,
    pub sigma2: pallas::Base,
}

impl ConsensusProposalRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let epoch_palas = pallas::Base::from(self.epoch);
        let (pub_x, pub_y) = self.public_key.xy();
        let value_coords = self.value_commit.to_affine().coordinates().unwrap();
        let new_serial_coords = self.new_serial_commit.to_affine().coordinates().unwrap();
        let reward_pallas = pallas::Base::from(REWARD);
        let new_value_coords = self.new_value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            self.nullifier.inner(),
            epoch_palas,
            pub_x,
            pub_y,
            self.merkle_root.inner(),
            *value_coords.x(),
            *value_coords.y(),
            *new_serial_coords.x(),
            *new_serial_coords.y(),
            reward_pallas,
            *new_value_coords.x(),
            *new_value_coords.y(),
            self.new_coin.inner(),
            self.mu_y,
            self.y,
            self.mu_rho,
            self.rho,
            self.sigma1,
            self.sigma2,
            HEADSTART,
        ]
    }
}

/// Struct holding necessary information to build a proposal transaction.
pub struct ConsensusProposalCallBuilder {
    /// `ConsensusOwnCoin` we're given to use in this builder
    pub coin: ConsensusOwnCoin,
    /// Rewarded slot checkpoint
    pub slot_checkpoint: SlotCheckpoint,
    /// Extending fork last proposal/block hash
    pub fork_hash: blake3::Hash,
    /// Extending fork second to last proposal/block hash
    pub fork_previous_hash: blake3::Hash,
    /// Merkle tree of coins used to create inclusion proofs
    pub tree: MerkleTree,
    /// `Proposal_V1` zkas circuit ZkBinary
    pub proposal_zkbin: ZkBinary,
    /// Proving key for the `Proposal_V1` zk circuit
    pub proposal_pk: ProvingKey,
}

impl ConsensusProposalCallBuilder {
    pub fn build(&self) -> Result<ConsensusProposalCallDebris> {
        debug!("Building Consensus::ProposalBurnV1 contract call for proposal");
        let value = self.coin.note.value;
        assert!(value != 0);

        debug!("Building Consensus::ProposalV1 anonymous input");
        let leaf_position = self.coin.leaf_position;
        let root = self.tree.root(0).unwrap();
        let merkle_path = self.tree.authentication_path(leaf_position, &root).unwrap();
        let input = ConsensusBurnInputInfo {
            leaf_position,
            merkle_path,
            secret: self.coin.secret,
            note: self.coin.note.clone(),
            value_blind: pallas::Scalar::random(&mut OsRng),
        };

        debug!("Building anonymous output");
        let reward_blind = pallas::Scalar::random(&mut OsRng);
        let new_value_blind = input.value_blind + reward_blind;
        let new_coin_blind = pallas::Base::random(&mut OsRng);
        let output = ConsensusMintOutputInfo {
            value: self.coin.note.value + REWARD,
            epoch: 0,
            public_key: PublicKey::from_secret(self.coin.secret),
            value_blind: new_value_blind,
            serial: self.coin.note.serial,
            coin_blind: new_coin_blind,
        };
        debug!("Finished building output");

        debug!("Building Consensus::ProposalV1 contract call for proposal");
        let (proof, public_inputs) = create_proposal_proof(
            &self.proposal_zkbin,
            &self.proposal_pk,
            &input,
            &output,
            &self.slot_checkpoint,
            self.fork_hash,
            self.fork_previous_hash,
        )?;

        let input = ConsensusInput {
            epoch: self.coin.note.epoch,
            coin: self.coin.coin,
            value_commit: public_inputs.value_commit,
            nullifier: public_inputs.nullifier,
            merkle_root: public_inputs.merkle_root,
            signature_public: public_inputs.public_key,
        };

        // Encrypted note
        let note = ConsensusNote {
            serial: public_inputs.new_serial,
            value: output.value,
            epoch: 0,
            coin_blind: new_coin_blind,
            value_blind: new_value_blind,
            reward: REWARD,
            reward_blind,
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let output = ConsensusOutput {
            value_commit: public_inputs.new_value_commit,
            coin: public_inputs.new_coin,
            note: encrypted_note,
        };

        // We now fill this with necessary stuff
        let new_serial_commit = public_inputs.new_serial_commit;
        let slot = self.slot_checkpoint.slot;
        let vrf_proof = public_inputs.vrf_proof;
        let y = public_inputs.y;
        let rho = public_inputs.rho;
        let params = ConsensusProposalParamsV1 {
            input,
            output,
            reward: REWARD,
            reward_blind,
            new_serial_commit,
            slot,
            fork_hash: self.fork_hash,
            fork_previous_hash: self.fork_previous_hash,
            vrf_proof,
            y,
            rho,
        };
        let proofs = vec![proof];

        // Now we should have all the params, zk proofs and signature secret.
        // We return it all and let the caller deal with it.
        let debris =
            ConsensusProposalCallDebris { params, proofs, signature_secret: self.coin.secret };
        Ok(debris)
    }
}

pub fn create_proposal_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ConsensusBurnInputInfo,
    output: &ConsensusMintOutputInfo,
    slot_checkpoint: &SlotCheckpoint,
    _fork_hash: blake3::Hash,
    fork_previous_hash: blake3::Hash,
) -> Result<(Proof, ConsensusProposalRevealed)> {
    // TODO: fork_hash to be used as part of rank constrain in the proof
    // Proof parameters
    let nullifier = Nullifier::from(poseidon_hash([input.secret.inner(), input.note.serial]));
    let epoch = input.note.epoch;
    let epoch_pallas = pallas::Base::from(epoch);
    let value_pallas = pallas::Base::from(input.note.value);
    let value_commit = pedersen_commitment_u64(input.note.value, input.value_blind);
    let public_key = PublicKey::from_secret(input.secret);
    let (pub_x, pub_y) = public_key.xy();

    // Burnt coin and its merkle_root
    let coin = poseidon_hash([
        pub_x,
        pub_y,
        value_pallas,
        epoch_pallas,
        input.note.serial,
        input.note.coin_blind,
    ]);

    let merkle_root = {
        let position: u64 = input.leaf_position.into();
        let mut current = MerkleNode::from(coin);
        for (level, sibling) in input.merkle_path.iter().enumerate() {
            let level = level as u8;
            current = if position & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current
    };

    // New coin
    let new_serial = poseidon_hash([SERIAL_PREFIX, input.secret.inner(), input.note.serial]);
    let new_serial_blind = pallas::Scalar::random(&mut OsRng);
    let new_serial_commit = pedersen_commitment_base(new_serial, new_serial_blind);
    let new_value_commit = pedersen_commitment_u64(output.value, output.value_blind);
    let new_value_pallas = pallas::Base::from(output.value);
    let (new_pub_x, new_pub_y) = output.public_key.xy();

    let new_coin = Coin::from(poseidon_hash([
        new_pub_x,
        new_pub_y,
        new_value_pallas,
        pallas::Base::ZERO,
        new_serial,
        output.coin_blind,
    ]));

    let slot_pallas = pallas::Base::from(slot_checkpoint.slot);
    let seed = poseidon_hash([SEED_PREFIX, input.note.serial]);
    let mut vrf_input = Vec::with_capacity(32 + blake3::OUT_LEN + 32);
    vrf_input.extend_from_slice(&slot_checkpoint.previous_eta.to_repr());
    vrf_input.extend_from_slice(fork_previous_hash.as_bytes());
    vrf_input.extend_from_slice(&slot_pallas.to_repr());
    let vrf_proof = VrfProof::prove(input.secret, &vrf_input, &mut OsRng);
    let mut eta = [0u8; 64];
    eta[..blake3::OUT_LEN].copy_from_slice(vrf_proof.hash_output().as_bytes());
    let eta = pallas::Base::from_uniform_bytes(&eta);
    let mu_y = poseidon_hash([MU_Y_PREFIX, eta, slot_pallas]);
    let y = poseidon_hash([seed, mu_y]);
    let mu_rho = poseidon_hash([MU_RHO_PREFIX, eta, slot_pallas]);
    let rho = poseidon_hash([seed, mu_rho]);
    let (sigma1, sigma2) = (slot_checkpoint.sigma1, slot_checkpoint.sigma2);

    // Generate public inputs, witnesses and proof
    let public_inputs = ConsensusProposalRevealed {
        nullifier,
        epoch,
        public_key,
        merkle_root,
        value_commit,
        new_serial,
        new_serial_commit,
        new_value_commit,
        new_coin,
        vrf_proof,
        mu_y,
        y,
        mu_rho,
        rho,
        sigma1,
        sigma2,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.secret.inner())),
        Witness::Base(Value::known(input.note.serial)),
        Witness::Base(Value::known(pallas::Base::from(input.note.value))),
        Witness::Base(Value::known(epoch_pallas)),
        Witness::Base(Value::known(REWARD_PALLAS)),
        Witness::Scalar(Value::known(input.value_blind)),
        Witness::Base(Value::known(input.note.coin_blind)),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
        Witness::Scalar(Value::known(new_serial_blind)),
        Witness::Base(Value::known(new_pub_x)),
        Witness::Base(Value::known(new_pub_y)),
        Witness::Scalar(Value::known(output.value_blind)),
        Witness::Base(Value::known(output.coin_blind)),
        Witness::Base(Value::known(mu_y)),
        Witness::Base(Value::known(mu_rho)),
        Witness::Base(Value::known(sigma1)),
        Witness::Base(Value::known(sigma2)),
        Witness::Base(Value::known(HEADSTART)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
