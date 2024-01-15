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
    error::Error::CoinIsNotSlotProducer,
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_money_contract::{
    client::{ConsensusNote, ConsensusOwnCoin},
    model::{Coin, ConsensusInput, ConsensusOutput, POW_REWARD},
};
use darkfi_sdk::{
    blockchain::Slot,
    bridgetree::Hashable,
    crypto::{
        ecvrf::VrfProof, note::AeadEncryptedNote, pasta_prelude::*, pedersen_commitment_u64,
        poseidon_hash, Keypair, MerkleNode, MerkleTree, Nullifier, PublicKey, SecretKey,
    },
    pasta::{group::ff::FromUniformBytes, pallas},
};
use log::{debug, error, info};
use rand::rngs::OsRng;

use crate::{
    client::common::{ConsensusBurnInputInfo, ConsensusMintOutputInfo},
    model::{
        ConsensusProposalParamsV1, HEADSTART, MU_RHO_PREFIX, MU_Y_PREFIX, SECRET_KEY_PREFIX,
        SEED_PREFIX, SERIAL_PREFIX,
    },
};

pub struct ConsensusProposalCallDebris {
    /// Payload params
    pub params: ConsensusProposalParamsV1,
    /// ZK proofs
    pub proofs: Vec<Proof>,
    /// The new output keypair (used in the minted coin)
    pub keypair: Keypair,
    /// Secret key used to sign the transaction
    pub signature_secret: SecretKey,
}

pub struct ConsensusProposalRevealed {
    pub nullifier: Nullifier,
    pub epoch: u64,
    pub public_key: PublicKey,
    pub merkle_root: MerkleNode,
    pub input_value_commit: pallas::Point,
    pub reward: u64,
    pub output_value_commit: pallas::Point,
    pub output_coin: Coin,
    pub vrf_proof: VrfProof,
    pub mu_y: pallas::Base,
    pub y: pallas::Base,
    pub mu_rho: pallas::Base,
    pub rho: pallas::Base,
    pub sigma1: pallas::Base,
    pub sigma2: pallas::Base,
    pub headstart: pallas::Base,
}

impl ConsensusProposalRevealed {
    fn to_vec(&self) -> Vec<pallas::Base> {
        let (pub_x, pub_y) = self.public_key.xy();
        let input_value_coords = self.input_value_commit.to_affine().coordinates().unwrap();
        let output_value_coords = self.output_value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            self.nullifier.inner(),
            pallas::Base::from(self.epoch),
            pub_x,
            pub_y,
            self.merkle_root.inner(),
            *input_value_coords.x(),
            *input_value_coords.y(),
            pallas::Base::from(self.reward),
            *output_value_coords.x(),
            *output_value_coords.y(),
            self.output_coin.inner(),
            self.mu_y,
            self.y,
            self.mu_rho,
            self.rho,
            self.sigma1,
            self.sigma2,
            self.headstart,
        ]
    }
}

/// Struct holding necessary information to build a proposal transaction.
pub struct ConsensusProposalCallBuilder {
    /// `ConsensusOwnCoin` we're given to use in this builder
    pub owncoin: ConsensusOwnCoin,
    /// Rewarded slot
    pub slot: Slot,
    /// Extending fork last proposal/block hash
    pub fork_hash: blake3::Hash,
    /// Extending fork second to last proposal/block hash
    pub fork_previous_hash: blake3::Hash,
    /// Merkle tree of coins used to create inclusion proofs
    pub merkle_tree: MerkleTree,
    /// `Proposal_V1` zkas circuit ZkBinary
    pub proposal_zkbin: ZkBinary,
    /// Proving key for the `Proposal_V1` zk circuit
    pub proposal_pk: ProvingKey,
}

impl ConsensusProposalCallBuilder {
    pub fn build(&self) -> Result<ConsensusProposalCallDebris> {
        let input_value_blind = pallas::Scalar::random(&mut OsRng);
        let output_reward_blind = pallas::Scalar::random(&mut OsRng);

        self.build_with_params(input_value_blind, output_reward_blind)
    }

    pub fn build_with_params(
        &self,
        input_value_blind: pallas::Scalar,
        output_reward_blind: pallas::Scalar,
    ) -> Result<ConsensusProposalCallDebris> {
        info!("Building Consensus::ProposalBurnV1 contract call");
        assert!(self.owncoin.note.value != 0);

        debug!("Building Consensus::ProposalV1 anonymous input");
        let merkle_path = self.merkle_tree.witness(self.owncoin.leaf_position, 0).unwrap();

        let input = ConsensusBurnInputInfo {
            leaf_position: self.owncoin.leaf_position,
            merkle_path,
            secret: self.owncoin.secret,
            note: self.owncoin.note.clone(),
            value_blind: input_value_blind,
        };

        debug!("Building Consensus::ProposalV1 anonymous output");
        let output_value_blind = input.value_blind + output_reward_blind;

        // The output's secret key is derived from the old secret key
        let output_secret_key = poseidon_hash([SECRET_KEY_PREFIX, self.owncoin.secret.inner()]);
        let output_keypair = Keypair::new(SecretKey::from(output_secret_key));

        // The output's serial is derived from the old serial
        let output_serial =
            poseidon_hash([SERIAL_PREFIX, self.owncoin.secret.inner(), self.owncoin.note.serial]);

        let output = ConsensusMintOutputInfo {
            value: self.owncoin.note.value + POW_REWARD,
            epoch: 0, // We set the epoch as 0 here to eliminate a potential timelock
            public_key: output_keypair.public,
            value_blind: output_value_blind,
            serial: output_serial,
        };

        info!("Building Consensus::ProposalV1 VRF proof");
        let mut vrf_input = Vec::with_capacity(32 + blake3::OUT_LEN + 32);
        vrf_input.extend_from_slice(&self.slot.last_nonce.to_repr());
        vrf_input.extend_from_slice(self.fork_previous_hash.as_bytes());
        vrf_input.extend_from_slice(&pallas::Base::from(self.slot.id).to_repr());
        let vrf_proof = VrfProof::prove(input.secret, &vrf_input, &mut OsRng);

        info!("Building Consensus::ProposalV1 ZK proof");
        let (proof, public_inputs) = create_proposal_proof(
            &self.proposal_zkbin,
            &self.proposal_pk,
            &input,
            &output,
            &self.slot,
            &vrf_proof,
        )?;

        let tx_input = ConsensusInput {
            epoch: input.note.epoch,
            value_commit: public_inputs.input_value_commit,
            nullifier: public_inputs.nullifier,
            merkle_root: public_inputs.merkle_root,
            signature_public: public_inputs.public_key,
        };

        // Output's encrypted note
        let note = ConsensusNote {
            serial: output.serial,
            value: output.value,
            epoch: output.epoch,
            value_blind: output.value_blind,
            reward: POW_REWARD,
            reward_blind: output_reward_blind,
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let tx_output = ConsensusOutput {
            value_commit: public_inputs.output_value_commit,
            coin: public_inputs.output_coin,
            note: encrypted_note,
        };

        // Construct params
        let params = ConsensusProposalParamsV1 {
            input: tx_input,
            output: tx_output,
            reward: POW_REWARD,
            reward_blind: output_reward_blind,
            fork_hash: self.fork_hash,
            fork_previous_hash: self.fork_previous_hash,
            vrf_proof,
            y: public_inputs.y,
            rho: public_inputs.rho,
        };

        // Construct debris
        let debris = ConsensusProposalCallDebris {
            params,
            proofs: vec![proof],
            keypair: output_keypair,
            signature_secret: input.secret,
        };
        Ok(debris)
    }
}

fn create_proposal_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ConsensusBurnInputInfo,
    output: &ConsensusMintOutputInfo,
    slot: &Slot,
    vrf_proof: &VrfProof,
) -> Result<(Proof, ConsensusProposalRevealed)> {
    // TODO: fork_hash to be used as part of rank constrain in the proof
    // Calculate lottery parameters
    let seed = poseidon_hash([SEED_PREFIX, input.note.serial]);

    let mut eta = [0u8; 64];
    eta[..blake3::OUT_LEN].copy_from_slice(vrf_proof.hash_output().as_bytes());
    let eta = pallas::Base::from_uniform_bytes(&eta);

    let mu_y = poseidon_hash([MU_Y_PREFIX, eta, pallas::Base::from(slot.id)]);
    let y = poseidon_hash([seed, mu_y]);
    let mu_rho = poseidon_hash([MU_RHO_PREFIX, eta, pallas::Base::from(slot.id)]);
    let rho = poseidon_hash([seed, mu_rho]);

    // Verify coin is the slot block producer
    let value_pallas = pallas::Base::from(input.note.value);
    let shifted_target =
        slot.pid.sigma1 * value_pallas + slot.pid.sigma2 * value_pallas * value_pallas + HEADSTART;

    if y >= shifted_target {
        error!("MU_Y: {:?}", mu_y);
        error!("Y: {:?}", y);
        error!("TARGET: {:?}", shifted_target);
        return Err(CoinIsNotSlotProducer)
    }

    // Derive the input's nullifier
    let nullifier = Nullifier::from(poseidon_hash([input.secret.inner(), input.note.serial]));

    // Create the value commitment for the input
    let input_value_commit = pedersen_commitment_u64(input.note.value, input.value_blind);

    // Merkle inclusion proof for the input
    let public_key = PublicKey::from_secret(input.secret);
    let (pub_x, pub_y) = public_key.xy();

    let coin = poseidon_hash([
        pub_x,
        pub_y,
        pallas::Base::from(input.note.value),
        pallas::Base::from(input.note.epoch),
        input.note.serial,
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

    // Derive the new output coin
    let (output_x, output_y) = output.public_key.xy();
    let output_coin = Coin::from(poseidon_hash([
        output_x,
        output_y,
        pallas::Base::from(output.value),
        pallas::Base::from(output.epoch),
        output.serial,
    ]));

    // Create the ZK proof
    let public_inputs = ConsensusProposalRevealed {
        nullifier,
        epoch: input.note.epoch,
        public_key,
        merkle_root,
        input_value_commit,
        reward: POW_REWARD,
        output_value_commit: pedersen_commitment_u64(output.value, output.value_blind),
        output_coin,
        vrf_proof: *vrf_proof,
        mu_y,
        y,
        mu_rho,
        rho,
        sigma1: slot.pid.sigma1,
        sigma2: slot.pid.sigma2,
        headstart: HEADSTART,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.secret.inner())),
        Witness::Base(Value::known(input.note.serial)),
        Witness::Base(Value::known(pallas::Base::from(input.note.value))),
        Witness::Base(Value::known(pallas::Base::from(input.note.epoch))),
        Witness::Base(Value::known(pallas::Base::from(POW_REWARD))),
        Witness::Scalar(Value::known(input.value_blind)),
        Witness::Uint32(Value::known(u64::from(input.leaf_position).try_into().unwrap())),
        Witness::MerklePath(Value::known(input.merkle_path.clone().try_into().unwrap())),
        Witness::Scalar(Value::known(output.value_blind)),
        Witness::Base(Value::known(public_inputs.mu_y)),
        Witness::Base(Value::known(public_inputs.mu_rho)),
        Witness::Base(Value::known(public_inputs.sigma1)),
        Witness::Base(Value::known(public_inputs.sigma2)),
        Witness::Base(Value::known(public_inputs.headstart)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
