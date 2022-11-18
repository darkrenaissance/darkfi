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
        pedersen::{pedersen_commitment_base, pedersen_commitment_u64},
        poseidon_hash,
        util::mod_r_p,
        MerkleNode, PublicKey, SecretKey,
    },
    pasta::{arithmetic::CurveAffine, group::Curve, pallas},
    zk::{vm::ZkCircuit, vm_stack::Witness},
    zkas::ZkBinary,
};
use halo2_proofs::{arithmetic::Field, circuit::Value};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::debug;
use rand::rngs::OsRng;

use super::constants::{EPOCH_LENGTH, PRF_NULLIFIER_PREFIX};
use crate::{
    crypto::{proof::ProvingKey, Proof},
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
    /// coin1 sk position in merkle tree
    pub coin1_sk_pos: u32,
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
        let prefix_evl = pallas::Base::from(2);
        let prefix_pk = pallas::Base::from(4);
        let prefix_pk = pallas::Base::from(5);
        let zero = pallas::Base::zero();
        // pk
        let pk_msg = [prefix_pk, self.coin1_sk_root, self.coin1_timestamp, zero];
        let pk = poseidon_hash(pk_msg);
        // rho
        let rho_msg = [prefix_evl, self.coin1_sk_root, self.coin1_nonce, zero];
        let c2_rho = poseidon_hash(rho_msg);
        // coin 1-2 cm/commitment
        let c1_cm = self.coin1_commitment.to_affine().coordinates().unwrap();
        let c2_cm = self.coin2_commitment.to_affine().coordinates().unwrap();
        // lottery seed
        let seed_msg = [self.coin1_sk_root.inner(), self.nonce];
        let seed = poseidon_hash(seed_msg);
        // y
        let y = pedersen_commitment_base(seed, mod_r_p(self.y_mu));
        let y_coords = y.to_affine().coordinates().unwrap();
        // rho
        let rho = pedersen_commitment_base(seed, mod_r_p(self.rho_mu));
        let rho_coord = rho.to_affine().coordinates().unwrap();
        vec![
            pk,
            c2_rho,
            *c1_cm.x(),
            *c1_cm.y(),
            *c2_cm.x(),
            *c2_cm.y(),
            self.coin1_commitment_root.inner(),
            self.coin1_sk_root.inner(),
            self.sn,
            *y_coords.x(),
            *y_coords.y(),
            *rho_coord.x(),
            *rho_coord.y(),
        ]
    }

    /// Try to create a ZK proof of consensus leadership
    pub fn create_lead_proof(&self, pk: &ProvingKey) -> Result<Proof> {
        let bincode = include_bytes!("../../proof/lead.zk.bin");
        let zkbin = ZkBinary::decode(bincode)?;
        let prover_witnesses = vec![
            Witness::Base(Value::known(self.coin1_commitment_merkle_path)),
            Witness::Base(Value::known(self.idx)),
            Witness::Base(Value::known(self.coin1_sk_pos)),
            Witness::Base(Value::known(self.secret_key.inner())),
            Witness::Base(Value::known(self.coin1_sk_root.inner())),
            Witness::Base(Value::known(self.coin1_sk_merkle_path)),
            Witness::Base(Value::known(self.tau)),
            Witness::Base(Value::known(self.nonce)),
            Witness::Base(Value::known(self.coin1_blind)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Base(Value::known(self.coin2_blind)),
            Witness::Base(Value::known(mod_r_p(self.rho_mu))),
            Witness::Base(Value::known(mod_r_p(self.y_mu))),
            Witness::Base(Value::known(self.sigma1)),
            Witness::Base(Value::known(self.sigma2)),
        ];
        let circuit = ZkCircuit::new(prover_witnesses, zkbin.clone());
        Ok(Proof::create(pk, &[circuit], &self.public_inputs(), &mut OsRng)?)
    }
}

/// This struct holds the secrets for creating LeadCoins during one epoch.
pub struct LeadCoinSecrets {
    pub secret_keys: Vec<SecretKey>,
    pub merkle_roots: Vec<MerkleNode>,
    pub merkle_paths: Vec<[MerkleNode; MERKLE_DEPTH_LEADCOIN]>,
}

impl LeadCoinSecrets {
    /// Generate epoch coins secret keys.
    /// First clot coin secret key is sampled at random, while the secret keys of the
    /// remaining slots derive from the previous slot secret.
    /// Clarification:
    /// ```plaintext
    /// sk[0] -> random,
    /// sk[1] -> derive_function(sk[0]),
    /// ...
    /// sk[n] -> derive_function(sk[n-1]),
    /// ```
    pub fn generate() -> Self {
        let mut tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(EPOCH_LENGTH);
        let mut sks = Vec::with_capacity(EPOCH_LENGTH);
        let mut root_sks = Vec::with_capacity(EPOCH_LENGTH);
        let mut path_sks = Vec::with_capacity(EPOCH_LENGTH);

        let mut prev_sk = SecretKey::from(pallas::Base::one());

        for i in 0..EPOCH_LENGTH {
            let secret = if i == 0 {
                pedersen_commitment_u64(1, pallas::Scalar::random(&mut OsRng))
            } else {
                pedersen_commitment_u64(1, mod_r_p(prev_sk.inner()))
            };

            let secret_coords = secret.to_affine().coordinates().unwrap();
            let secret_msg = [*secret_coords.x(), *secret_coords.y()];
            let secret_key = SecretKey::from(poseidon_hash(secret_msg));

            sks.push(secret_key);
            prev_sk = secret_key;

            let node = MerkleNode::from(secret_key.inner());
            tree.append(&node);
            let leaf_pos = tree.witness().unwrap();
            let root = tree.root(0).unwrap();
            let path = tree.authentication_path(leaf_pos, &root).unwrap();

            root_sks.push(root);
            path_sks.push(path.try_into().unwrap());
        }

        Self { secret_keys: sks, merkle_roots: root_sks, merkle_paths: path_sks }
    }
}
