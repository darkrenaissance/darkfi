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
    client::{MoneyNote, OwnCoin},
    model::{ConsensusStakeParamsV1, ConsensusUnstakeParamsV1, Input, Output, StakeInput},
};
use darkfi_sdk::{
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, pedersen_commitment_u64, poseidon_hash,
        MerkleTree, PublicKey, SecretKey, CONSENSUS_CONTRACT_ID, DARK_TOKEN_ID,
    },
    incrementalmerkletree::Tree,
    pasta::pallas,
};
use log::{debug, info};
use rand::rngs::OsRng;

use crate::{
    client::{
        stake_v1::{create_stake_mint_proof, TransactionBuilderOutputInfo as StakeTBOI},
        unstake_v1::{create_unstake_burn_proof, TransactionBuilderInputInfo as UnstakeTBII},
    },
    model::{
        ConsensusRewardParamsV1, HEADSTART, MU_RHO_PREFIX, MU_Y_PREFIX, REWARD, REWARD_PALLAS,
        SEED_PREFIX, ZERO,
    },
};

pub struct ConsensusProposalCallDebris {
    pub unstake_params: ConsensusUnstakeParamsV1,
    pub unstake_proofs: Vec<Proof>,
    pub reward_params: ConsensusRewardParamsV1,
    pub reward_proofs: Vec<Proof>,
    pub stake_params: ConsensusStakeParamsV1,
    pub stake_proofs: Vec<Proof>,
    pub signature_secret: SecretKey,
}

pub struct ConsensusProposalRewardRevealed {
    pub value_commit: pallas::Point,
    pub new_value_commit: pallas::Point,
    pub mu_y: pallas::Base,
    pub y: pallas::Base,
    pub mu_rho: pallas::Base,
    pub rho: pallas::Base,
    pub sigma1: pallas::Base,
    pub sigma2: pallas::Base,
}

impl ConsensusProposalRewardRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let value_coords = self.value_commit.to_affine().coordinates().unwrap();
        let new_value_coords = self.new_value_commit.to_affine().coordinates().unwrap();

        // NOTE: It's important to keep these in the same order
        // as the `constrain_instance` calls in the zkas code.
        vec![
            *value_coords.x(),
            *value_coords.y(),
            *new_value_coords.x(),
            *new_value_coords.y(),
            self.mu_y,
            self.y,
            self.mu_rho,
            self.rho,
            self.sigma1,
            self.sigma2,
        ]
    }
}

/// Struct holding necessary information to build a proposal transaction.
pub struct ConsensusProposalCallBuilder {
    /// `OwnCoin` we're given to use in this builder
    pub coin: OwnCoin,
    /// Recipient's public key
    pub recipient: PublicKey,
    /// Rewarded slot checkpoint
    pub slot_checkpoint: SlotCheckpoint,
    /// Merkle tree of coins used to create inclusion proofs
    pub tree: MerkleTree,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
    /// `Reward_V1` zkas circuit ZkBinary
    pub reward_zkbin: ZkBinary,
    /// Proving key for the `Reward_V1` zk circuit
    pub reward_pk: ProvingKey,
    /// `Mint_V1` zkas circuit ZkBinary
    pub mint_zkbin: ZkBinary,
    /// Proving key for the `Mint_V1` zk circuit
    pub mint_pk: ProvingKey,
}

impl ConsensusProposalCallBuilder {
    pub fn build(&self) -> Result<ConsensusProposalCallDebris> {
        debug!("Building Consensus::UnstakeV1 contract call for proposal");
        let value = self.coin.note.value;
        let token_id = self.coin.note.token_id;
        assert!(value != 0);
        assert!(token_id == *DARK_TOKEN_ID);

        debug!("Building anonymous input for proposal");
        let leaf_position = self.coin.leaf_position;
        let root = self.tree.root(0).unwrap();
        let merkle_path = self.tree.authentication_path(leaf_position, &root).unwrap();
        let input = UnstakeTBII {
            leaf_position,
            merkle_path,
            secret: self.coin.secret,
            note: self.coin.note.clone(),
        };
        debug!("Finished building input for proposal");

        let value_blind = pallas::Scalar::random(&mut OsRng);
        let token_blind = pallas::Scalar::random(&mut OsRng);
        let signature_secret = SecretKey::random(&mut OsRng);
        let user_data_blind = pallas::Base::random(&mut OsRng);
        info!("Creating unstake burn proof for input for proposal");
        let (proof, public_inputs) = create_unstake_burn_proof(
            &self.burn_zkbin,
            &self.burn_pk,
            &input,
            value_blind,
            token_blind,
            user_data_blind,
            signature_secret,
        )?;

        let input = Input {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
            nullifier: public_inputs.nullifier,
            merkle_root: public_inputs.merkle_root,
            spend_hook: public_inputs.spend_hook,
            user_data_enc: public_inputs.user_data_enc,
            signature_public: public_inputs.signature_public,
        };

        // We now fill this with necessary stuff
        let unstake_params = ConsensusUnstakeParamsV1 { token_blind, input: input.clone() };
        let unstake_proofs = vec![proof];
        let unstake_input = input;

        debug!("Building Consensus::StakeV1 contract call for proposal");
        let new_value = value + REWARD;
        let nullifier = public_inputs.nullifier;
        let merkle_root = public_inputs.merkle_root;
        let signature_public = public_inputs.signature_public;

        debug!("Building anonymous output for proposal");
        let output = StakeTBOI { value: new_value, token_id, public_key: self.recipient };
        debug!("Finished building output for proposal");

        let serial = pallas::Base::random(&mut OsRng);
        let spend_hook = CONSENSUS_CONTRACT_ID.inner();
        let user_data = pallas::Base::random(&mut OsRng);
        let coin_blind = pallas::Base::random(&mut OsRng);

        info!("Creating stake mint proof for output for proposal");
        let (proof, public_inputs) = create_stake_mint_proof(
            &self.mint_zkbin,
            &self.mint_pk,
            &output,
            value_blind,
            token_blind,
            serial,
            spend_hook,
            user_data,
            coin_blind,
        )?;

        // Encrypted note
        let note = MoneyNote {
            serial,
            value: output.value,
            token_id: output.token_id,
            spend_hook,
            user_data,
            coin_blind,
            value_blind,
            token_blind,
            memo: vec![],
        };

        let encrypted_note = AeadEncryptedNote::encrypt(&note, &output.public_key, &mut OsRng)?;

        let output = Output {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
            coin: public_inputs.coin,
            note: encrypted_note,
        };

        let input = StakeInput {
            token_blind,
            value_commit: public_inputs.value_commit,
            nullifier,
            merkle_root,
            signature_public,
        };

        // We now fill this with necessary stuff
        let stake_params = ConsensusStakeParamsV1 { input: input.clone(), output: output.clone() };
        let stake_proofs = vec![proof];
        let stake_input = input;

        debug!("Building Consensus::RewardV1 contract call for proposal");
        let coin = self.coin.coin.inner();
        let secret_key = self.coin.secret.inner();
        let (proof, public_inputs) = create_proposal_reward_proof(
            &self.reward_zkbin,
            &self.reward_pk,
            &self.slot_checkpoint,
            coin,
            secret_key,
            value,
            value_blind,
        )?;

        // We now fill this with necessary stuff
        let slot = self.slot_checkpoint.slot;
        let y = public_inputs.y;
        let rho = public_inputs.rho;
        let reward_params =
            ConsensusRewardParamsV1 { unstake_input, stake_input, output, slot, y, rho };
        let reward_proofs = vec![proof];

        // Now we should have all the params, zk proofs and signature secret.
        // We return it all and let the caller deal with it.
        let debris = ConsensusProposalCallDebris {
            unstake_params,
            unstake_proofs,
            reward_params,
            reward_proofs,
            stake_params,
            stake_proofs,
            signature_secret,
        };
        Ok(debris)
    }
}

pub fn create_proposal_reward_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    slot_checkpoint: &SlotCheckpoint,
    coin: pallas::Base,
    secret_key: pallas::Base,
    value: u64,
    value_blind: pallas::Scalar,
) -> Result<(Proof, ConsensusProposalRewardRevealed)> {
    // Proof parameters
    let value_commit = pedersen_commitment_u64(value, value_blind);
    let new_value_commit = pedersen_commitment_u64(value + REWARD, value_blind);
    let slot_pallas = pallas::Base::from(slot_checkpoint.slot);
    let seed = poseidon_hash([SEED_PREFIX, coin, secret_key, ZERO]);
    let mu_y = poseidon_hash([MU_Y_PREFIX, slot_checkpoint.eta, slot_pallas]);
    let y = poseidon_hash([seed, mu_y]);
    let mu_rho = poseidon_hash([MU_RHO_PREFIX, slot_checkpoint.eta, slot_pallas]);
    let rho = poseidon_hash([seed, mu_rho]);
    let (sigma1, sigma2) = (slot_checkpoint.sigma1, slot_checkpoint.sigma2);

    // Generate public inputs, witnesses and proof
    let public_inputs = ConsensusProposalRewardRevealed {
        value_commit,
        new_value_commit,
        mu_y,
        y,
        mu_rho,
        rho,
        sigma1,
        sigma2,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(coin)),
        Witness::Base(Value::known(secret_key)),
        Witness::Base(Value::known(pallas::Base::from(value))),
        Witness::Base(Value::known(REWARD_PALLAS)),
        Witness::Scalar(Value::known(value_blind)),
        Witness::Base(Value::known(mu_y)),
        Witness::Base(Value::known(mu_rho)),
        Witness::Base(Value::known(HEADSTART)),
        Witness::Base(Value::known(sigma1)),
        Witness::Base(Value::known(sigma2)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
