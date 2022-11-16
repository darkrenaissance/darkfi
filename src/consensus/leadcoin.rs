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

use darkfi_sdk::{
    crypto::{
        pedersen::pedersen_commitment_base, poseidon_hash, util::mod_r_p, MerkleNode, PublicKey,
        SecretKey,
    },
    pasta::{arithmetic::CurveAffine, group::Curve, pallas},
};
use halo2_proofs::{arithmetic::Field, circuit::Value};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::debug;
use rand::rngs::OsRng;

use super::constants::PRF_NULLIFIER_PREFIX;
use crate::{
    crypto::{proof::ProvingKey, Proof},
    zk::circuit::LeadContract,
    Result,
};

pub const MERKLE_DEPTH_LEADCOIN: usize = 32;
pub const MERKLE_DEPTH: u8 = 32;

// TODO: Unify item names with the names in the ZK proof (those are more descriptive)
/// Structure representing the consensus leader coin
#[derive(Debug, Clone, Copy)]
pub struct LeadCoin {
    /// Coin's stake value
    pub value: u64,
    /// Commitment for coin1
    pub coin1_commitment: pallas::Point,
    /// Commitment for coin2 (poured coin)
    pub coin2_commitment: pallas::Point,
    /// Coin index
    pub idx: u32,
    /// Coin slot ID,
    pub sl: pallas::Base,
    /// Coin timestamp
    pub tau: pallas::Base,
    /// Coin nonce
    pub nonce: pallas::Base,
    /// Coin nonce's commitment
    pub nonce_cm: pallas::Base,
    /// Coin's serial number
    pub sn: pallas::Base,
    /// Merkle root of coin1 commitment
    pub coin1_commitment_root: MerkleNode,
    /// Merkle root of the `coin1` secret key
    pub coin1_sk_root: MerkleNode,
    /// Merkle path to the coin1's commitment
    pub coin1_commitment_merkle_path: [MerkleNode; MERKLE_DEPTH_LEADCOIN],
    /// Merkle path to the secret key of `coin1`
    pub coin1_sk_merkle_path: [MerkleNode; MERKLE_DEPTH_LEADCOIN],
    /// coin1 commitment blinding factor
    pub coin1_blind: pallas::Scalar,
    /// coin2 commitment blinding factor
    pub coin2_blind: pallas::Scalar,
    /// Leader election nonce derived from eta at onset of epoch
    pub y_mu: pallas::Base,
    /// Leader election nonce derived from eta at onset of epoch
    pub rho_mu: pallas::Base,
    /// First coefficient in 1-term T (target function) approximation.
    /// NOTE: sigma1 and sigma2 are not the capital sigma from the paper, but
    /// the whole coefficient multiplied with absolute stake.
    pub sigma1: pallas::Base,
    /// Second coefficient in 2-term T (target function) approximation.
    pub sigma2: pallas::Base,
    /// Coin's secret key
    pub secret_key: SecretKey,
}

impl LeadCoin {
    /// Create a new `LeadCoin` object using given parameters.
    pub fn new(
        // wtf is eta and why is it not in the zk proof?
        eta: pallas::Base,
        // First coefficient in 1-term T (target function) approximation.
        sigma1: pallas::Base,
        // Second coefficient in 2-term T (target function) approximation.
        sigma2: pallas::Base,
        // Stake value
        value: u64,
        // Slot index in the epock
        slot_index: usize,
        // Merkle root of the `coin_1` secret key in the Merkle tree of secret keys
        coin1_sk_root: MerkleNode,
        // Merkle path to the secret key of `coin_1` in the Merkle tree of secret keys
        coin1_sk_merkle_path: [MerkleNode; MERKLE_DEPTH_LEADCOIN],
        // what's seed supposed to be?
        seed: u64,
        // what is this SecretKey representing?
        secret_key: SecretKey,
        // Merkle tree of coin commitments
        coin_commitment_tree: &mut BridgeTree<MerkleNode, MERKLE_DEPTH>,
    ) -> Self {
        // Generate random blinding values for commitments:
        let coin1_blind = pallas::Scalar::random(&mut OsRng);
        let coin2_blind = pallas::Scalar::random(&mut OsRng);

        // Derive a public key from the secret key
        let public_key = PublicKey::from_secret(secret_key);
        let (coin_pk_x, coin_pk_y) = public_key.xy();
        debug!("coin_pk[{}] x: {:?}", slot_index, coin_pk_x);
        debug!("coin_pk[{}] y: {:?}", slot_index, coin_pk_y);

        // Derive a nullifier
        let sn_msg = [
            pallas::Base::from(seed),
            coin1_sk_root.inner(),
            pallas::Base::zero(),
            pallas::Base::one(),
        ];
        let c_sn = poseidon_hash(sn_msg);

        // Derive input for the commitment of coin1
        let coin1_commit_msg = [
            pallas::Base::from(PRF_NULLIFIER_PREFIX),
            coin_pk_x,
            coin_pk_y,
            pallas::Base::from(value),
            pallas::Base::from(seed),
            pallas::Base::one(),
        ];
        let coin1_commit_v = poseidon_hash(coin1_commit_msg);

        // Create commitment to coin1
        let coin1_commitment = pedersen_commitment_base(coin1_commit_v, coin1_blind);
        // Hash its coordinates to get a base field element
        let c1_cm_coords = coin1_commitment.to_affine().coordinates().unwrap();
        let c1_base_msg = [*c1_cm_coords.x(), *c1_cm_coords.y()];
        let coin1_commitment_base = poseidon_hash(c1_base_msg);

        // Append the element to the Merkle tree
        coin_commitment_tree.append(&MerkleNode::from(coin1_commitment_base));
        let leaf_pos = coin_commitment_tree.witness().unwrap();
        let coin1_commitment_root = coin_commitment_tree.root(0).unwrap();
        let coin1_commitment_merkle_path =
            coin_commitment_tree.authentication_path(leaf_pos, &coin1_commitment_root).unwrap();

        // Derive the nonce for coin2
        let coin2_nonce_msg = [
            pallas::Base::from(seed),
            coin1_sk_root.inner(),
            pallas::Base::one(),
            pallas::Base::one(),
        ];
        let coin2_seed = poseidon_hash(coin2_nonce_msg);
        debug!("coin2_seed[{}]: {:?}", slot_index, coin2_seed);

        // Derive input for the commitment of coin2
        let coin2_commit_msg = [
            pallas::Base::from(PRF_NULLIFIER_PREFIX),
            coin_pk_x,
            coin_pk_y,
            pallas::Base::from(value),
            coin2_seed,
            pallas::Base::one(),
        ];
        let coin2_commit_v = poseidon_hash(coin2_commit_msg);

        // Create commitment to coin2
        let coin2_commitment = pedersen_commitment_base(coin2_commit_v, coin2_blind);

        // Derive election seeds
        let (y_mu, rho_mu) = Self::election_seeds(eta, pallas::Base::from(slot_index as u64));

        // Return the object
        Self {
            value,
            coin1_commitment,
            coin2_commitment,
            // TODO: Should be abs slot
            idx: u32::try_from(usize::from(leaf_pos)).unwrap(),
            sl: pallas::Base::from(slot_index as u64),
            // Assume tau is sl for simplicity
            tau: pallas::Base::from(slot_index as u64),
            nonce: pallas::Base::from(seed),
            nonce_cm: coin2_seed,
            sn: c_sn,
            coin1_commitment_root,
            coin1_sk_root,
            coin1_commitment_merkle_path: coin1_commitment_merkle_path.try_into().unwrap(),
            coin1_sk_merkle_path,
            coin1_blind,
            coin2_blind,
            y_mu,
            rho_mu,
            sigma1,
            sigma2,
            secret_key,
        }
    }

    /// Derive election seeds from given parameters
    fn election_seeds(eta: pallas::Base, slot: pallas::Base) -> (pallas::Base, pallas::Base) {
        let election_seed_nonce = pallas::Base::from(3);
        let election_seed_lead = pallas::Base::from(22);

        // mu_y
        let lead_msg = [election_seed_lead, eta, slot];
        let lead_mu = poseidon_hash(lead_msg);

        // mu_rho
        let nonce_msg = [election_seed_nonce, eta, slot];
        let nonce_mu = poseidon_hash(nonce_msg);

        (lead_mu, nonce_mu)
    }

    /// Create a vector of `pallas::Base` elements from the `LeadCoin` to be
    /// used as public inputs for the ZK proof.
    pub fn public_inputs(&self) -> Vec<pallas::Base> {
        let lottery_msg_input = [self.coin1_sk_root.inner(), self.nonce];
        let lottery_msg = poseidon_hash(lottery_msg_input);

        let y = pedersen_commitment_base(lottery_msg, mod_r_p(self.y_mu));
        let y_coords = y.to_affine().coordinates().unwrap();
        let y_coords = [*y_coords.x(), *y_coords.y()];
        let y = poseidon_hash(y_coords);

        let pubkey = PublicKey::from_secret(self.secret_key);
        let (pub_x, pub_y) = pubkey.xy();

        vec![self.nonce_cm, pub_x, pub_y, y]
    }

    /// Try to create a ZK proof of consensus leadership
    pub fn create_lead_proof(&self, pk: &ProvingKey) -> Result<Proof> {
        // Initialize circuit with witnesses
        let lottery_msg_input = [self.coin1_sk_root.inner(), self.nonce];
        let lottery_msg = poseidon_hash(lottery_msg_input);
        let rho = pedersen_commitment_base(lottery_msg, mod_r_p(self.rho_mu));

        let circuit = LeadContract {
            coin1_commit_merkle_path: Value::known(self.coin1_commitment_merkle_path),
            coin1_commit_root: Value::known(self.coin1_commitment_root.inner()),
            coin1_commit_leaf_pos: Value::known(self.idx),
            coin1_sk: Value::known(self.secret_key.inner()),
            coin1_sk_root: Value::known(self.coin1_sk_root.inner()),
            coin1_sk_merkle_path: Value::known(self.coin1_sk_merkle_path),
            coin1_timestamp: Value::known(self.tau),
            coin1_nonce: Value::known(self.nonce),
            coin1_blind: Value::known(self.coin1_blind),
            coin1_serial: Value::known(self.sn),
            coin1_value: Value::known(pallas::Base::from(self.value)),
            coin2_blind: Value::known(self.coin2_blind),
            coin2_commit: Value::known(self.coin2_commitment),
            rho_mu: Value::known(mod_r_p(self.rho_mu)),
            y_mu: Value::known(mod_r_p(self.y_mu)),
            sigma1: Value::known(self.sigma1),
            sigma2: Value::known(self.sigma2),
            rho: Value::known(rho),
        };

        Ok(Proof::create(pk, &[circuit], &self.public_inputs(), &mut OsRng)?)
    }
}
