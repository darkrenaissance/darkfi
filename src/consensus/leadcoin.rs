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
        MerkleNode, SecretKey,
    },
    pasta::{arithmetic::CurveAffine, group::Curve, pallas},
};
use halo2_proofs::{arithmetic::Field, circuit::Value};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::debug;
use rand::rngs::OsRng;

use super::constants::EPOCH_LENGTH;
use crate::{
    consensus::{EncryptedTxRcpt, TransferStx, TxRcpt},
    crypto::{proof::ProvingKey, Proof},
    zk::{vm::ZkCircuit, vm_stack::Witness},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use log::info;

pub const MERKLE_DEPTH_LEADCOIN: usize = 32;
pub const MERKLE_DEPTH: u8 = 32;
pub const ZERO: pallas::Base = pallas::Base::zero();
pub const ONE: pallas::Base = pallas::Base::one();
pub const PREFIX_EVL: u64 = 2;
pub const PREFIX_SEED: u64 = 3;
pub const PREFIX_CM: u64 = 4;
pub const PREFIX_PK: u64 = 5;
pub const PREFIX_SN: u64 = 6;

// TODO: Unify item names with the names in the ZK proof (those are more descriptive)
/// Structure representing the consensus leader coin
#[derive(Debug, Clone, Copy)]
pub struct LeadCoin {
    /// Coin's stake value
    pub value: u64,
    /// Commitment for coin1
    pub coin1_commitment: pallas::Point,
    /// Commitment for coin2 (rcpt coin)
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
    /// Merkle root of coin1 commitment
    pub coin1_commitment_root: MerkleNode,
    /// coin1 sk
    pub coin1_sk: pallas::Base,
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
    /// Coin's secret key
    pub secret_key: SecretKey,
}

impl LeadCoin {
    /// Create a new `LeadCoin` object using given parameters.
    pub fn new(
        // emulation of global random oracle output from previous epoch randomness.
        eta: pallas::Base,
        // Stake value
        value: u64,
        // Slot index in the epoch
        slot_index: u64,
        // coin1 sk
        coin1_sk: pallas::Base,
        // Merkle root of the `coin_1` secret key in the Merkle tree of secret keys
        coin1_sk_root: MerkleNode,
        // sk pos
        coin1_sk_pos: usize,
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
        let tau = pallas::Base::from(slot_index);
        // pk
        let pk_msg =
            [pallas::Base::from(PREFIX_PK), coin1_sk_root.inner(), tau, pallas::Base::from(ZERO)];
        let pk = poseidon_hash(pk_msg);
        // Derive the nonce for coin2
        let coin2_nonce_msg = [
            pallas::Base::from(PREFIX_EVL),
            coin1_sk_root.inner(),
            pallas::Base::from(seed),
            pallas::Base::from(ZERO),
        ];
        let coin2_seed = poseidon_hash(coin2_nonce_msg);
        debug!("coin2_seed[{}]: {:?}", slot_index, coin2_seed);
        // Derive input for the commitment of coin1
        let coin1_commit_msg = [
            pallas::Base::from(PREFIX_CM),
            pk,
            pallas::Base::from(value),
            pallas::Base::from(seed),
        ];
        // Create commitment to coin1
        let coin1_commit_v = poseidon_hash(coin1_commit_msg);
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
        // Derive input for the commitment of coin2
        let coin2_commit_msg = [
            pallas::Base::from(PREFIX_CM),
            pk,
            pallas::Base::from(value),
            pallas::Base::from(coin2_seed),
        ];
        let coin2_commit_v = poseidon_hash(coin2_commit_msg);
        // Create commitment to coin2
        let coin2_commitment = pedersen_commitment_base(coin2_commit_v, coin2_blind);
        // Derive election seeds
        let (y_mu, rho_mu) = Self::election_seeds(eta, pallas::Base::from(slot_index));
        // Return the object
        Self {
            value,
            coin1_commitment,
            coin2_commitment,
            // TODO: Should be abs slot
            idx: u32::try_from(usize::from(leaf_pos)).unwrap(),
            sl: pallas::Base::from(slot_index),
            // Assume tau is sl for simplicity
            tau,
            nonce: pallas::Base::from(seed),
            nonce_cm: coin2_seed,
            coin1_commitment_root,
            coin1_sk,
            coin1_sk_root,
            coin1_sk_pos: u32::try_from(usize::from(coin1_sk_pos)).unwrap(),
            coin1_commitment_merkle_path: coin1_commitment_merkle_path.try_into().unwrap(),
            coin1_sk_merkle_path,
            coin1_blind,
            coin2_blind,
            y_mu,
            rho_mu,
            secret_key,
        }
    }

    fn sn(&self) -> pallas::Base {
        let sn_msg = [
            pallas::Base::from(PREFIX_SN),
            self.coin1_sk_root.inner(),
            self.nonce,
            pallas::Base::from(ZERO),
        ];
        poseidon_hash(sn_msg)
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
        let zero = pallas::Base::zero();
        let prefix_evl = pallas::Base::from(2);
        let prefix_seed = pallas::Base::from(3);
        let prefix_pk = pallas::Base::from(5);

        // pk
        let pk = self.pk();
        // rho
        let c2_rho = self.derived_rho();
        // coin 1-2 cm/commitment
        let c1_cm = self.coin1_commitment.to_affine().coordinates().unwrap();
        let c2_cm = self.coin2_commitment.to_affine().coordinates().unwrap();
        // lottery seed
        let seed_msg = [
            pallas::Base::from(PREFIX_SEED),
            self.coin1_sk_root.inner(),
            self.nonce,
            pallas::Base::from(ZERO),
        ];
        let seed = poseidon_hash(seed_msg);
        // y
        let y = pedersen_commitment_base(seed, mod_r_p(self.y_mu));
        let y_coords = y.to_affine().coordinates().unwrap();
        // rho
        let rho = pedersen_commitment_base(seed, mod_r_p(self.rho_mu));
        let rho_coord = rho.to_affine().coordinates().unwrap();
        vec![
            pk,
            *c1_cm.x(),
            *c1_cm.y(),
            *c2_cm.x(),
            *c2_cm.y(),
            self.coin1_commitment_root.inner(),
            self.coin1_sk_root.inner(),
            self.sn(),
            *y_coords.x(),
            *y_coords.y(),
            *rho_coord.x(),
            *rho_coord.y(),
        ]
    }
    /// calculate coin public key: hash of root coin secret key
    /// and timestmap.
    pub fn pk(&self) -> pallas::Base {
        let pk_msg = [
            pallas::Base::from(PREFIX_PK),
            self.coin1_sk_root.inner(),
            self.tau,
            pallas::Base::from(ZERO),
        ];
        let pk = poseidon_hash(pk_msg);
        pk
    }
    /// calculate derived coin nonce: hash of root coin secret key
    /// and old nonce
    pub fn derived_rho(&self) -> pallas::Base {
        let rho_msg = [
            pallas::Base::from(PREFIX_EVL),
            self.coin1_sk_root.inner(),
            self.nonce,
            pallas::Base::from(ZERO),
        ];
        let rho = poseidon_hash(rho_msg);
        rho
    }

    pub fn is_leader(&self, sigma1: pallas::Base, sigma2: pallas::Base) -> bool {
        let y_exp = [self.coin1_sk_root.inner(), self.nonce];
        let y_exp_hash = poseidon_hash(y_exp);
        let y_coords = pedersen_commitment_base(y_exp_hash, mod_r_p(self.y_mu))
            .to_affine()
            .coordinates()
            .unwrap();

        let y_coords = [*y_coords.x(), *y_coords.y()];
        let y = poseidon_hash(y_coords);

        let value = pallas::Base::from(self.value);
        let target = sigma1 * value + sigma2 * value * value;

        info!("Consensus::is_leader(): y = {:?}", y);
        info!("Consensus::is_leader(): T = {:?}", target);

        let first_winning = y < target;
        first_winning
    }

    /// calculated derived coin commitment
    pub fn derived_commitment(&self, blind: pallas::Scalar) -> pallas::Point {
        let pk = self.pk();
        let rho = self.derived_rho();
        let cm_in = [pallas::Base::from(PREFIX_CM), pk, pallas::Base::from(self.value), rho];
        let cm_v = poseidon_hash(cm_in);

        let cm = pedersen_commitment_base(cm_v, blind);
        cm
    }
    /// the new coin to be minted after the current coin is spent
    /// in lottery.
    pub fn derive_coin(&self, eta: pallas::Base, slot: u64) -> LeadCoin {
        let tau = pallas::Base::from(slot);
        let mut derived = self.clone();
        let pk = self.pk();
        let rho = self.derived_rho();
        let blind = pallas::Scalar::random(&mut OsRng);
        let cm = self.derived_commitment(blind);
        derived.nonce = rho;
        derived.coin1_commitment = derived.coin2_commitment;
        derived.coin2_commitment = cm;
        derived.coin1_blind = derived.coin2_blind;
        derived.coin2_blind = blind;
        // update random mau_y, mau_rho in case epoch is changed
        let (y_mu, rho_mu) = Self::election_seeds(eta, tau);
        derived.y_mu = y_mu;
        derived.rho_mu = rho_mu;
        derived
    }

    /// Try to create a ZK proof of consensus leadership
    pub fn create_lead_proof(&self,
                             sigma1: pallas::Base,
                             sigma2: pallas::Base,
                             pk: &ProvingKey) -> Result<Proof> {
        let bincode = include_bytes!("../../proof/lead.zk.bin");
        let zkbin = ZkBinary::decode(bincode)?;
        let witnesses = vec![
            Witness::MerklePath(Value::known(self.coin1_commitment_merkle_path)),
            Witness::Uint32(Value::known(self.idx)),
            Witness::Uint32(Value::known(self.coin1_sk_pos)),
            Witness::Base(Value::known(self.secret_key.inner())),
            Witness::Base(Value::known(self.coin1_sk_root.inner())),
            Witness::MerklePath(Value::known(self.coin1_sk_merkle_path)),
            Witness::Base(Value::known(self.tau)),
            Witness::Base(Value::known(self.nonce)),
            Witness::Scalar(Value::known(self.coin1_blind)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Scalar(Value::known(self.coin2_blind)),
            Witness::Scalar(Value::known(mod_r_p(self.rho_mu))),
            Witness::Scalar(Value::known(mod_r_p(self.y_mu))),
            Witness::Base(Value::known(sigma1)),
            Witness::Base(Value::known(sigma2)),
        ];
        let circuit = ZkCircuit::new(witnesses, zkbin.clone());
        Ok(Proof::create(pk, &[circuit], &self.public_inputs(), &mut OsRng)?)
    }

    pub fn create_xfer_proof(
        &self,
        pk: &ProvingKey,
        change_coin: TxRcpt,
        change_pk: pallas::Base, //change coin public key
        transfered_coin: TxRcpt,
        transfered_pk: pallas::Base, // recipient coin's public key
    ) -> Result<TransferStx> {
        assert!(change_coin.value + transfered_coin.value == self.value && self.value > 0);
        let bincode = include_bytes!("../../proof/tx.zk.bin");
        let zkbin = ZkBinary::decode(bincode)?;
        let retval = pallas::Base::from(change_coin.value);
        let xferval = pallas::Base::from(transfered_coin.value);
        let pos: u32 = self.idx;
        let value = pallas::Base::from(self.value);
        let witnesses = vec![
            // coin (1) burned coin
            Witness::Base(Value::known(self.coin1_commitment_root.inner())),
            Witness::Base(Value::known(self.coin1_sk_root.inner())),
            Witness::Base(Value::known(self.coin1_sk)),
            Witness::MerklePath(Value::known(self.coin1_sk_merkle_path)),
            Witness::Uint32(Value::known(self.coin1_sk_pos)),
            Witness::Base(Value::known(self.nonce)),
            Witness::Scalar(Value::known(self.coin1_blind)),
            Witness::Base(Value::known(value)),
            Witness::MerklePath(Value::known(self.coin1_commitment_merkle_path)),
            Witness::Uint32(Value::known(pos)),
            Witness::Base(Value::known(self.sn())),
            // coin (3)
            Witness::Base(Value::known(change_pk)),
            Witness::Base(Value::known(change_coin.rho)),
            Witness::Scalar(Value::known(change_coin.opening)),
            Witness::Base(Value::known(retval)),
            // coin (4)
            Witness::Base(Value::known(transfered_pk)),
            Witness::Base(Value::known(transfered_coin.rho)),
            Witness::Scalar(Value::known(transfered_coin.opening)),
            Witness::Base(Value::known(xferval)),
        ];
        let circuit = ZkCircuit::new(witnesses, zkbin.clone());
        let proof = Proof::create(pk, &[circuit], &self.public_inputs(), &mut OsRng)?;
        let cm3_msg_in = [
            pallas::Base::from(PREFIX_CM),
            change_pk,
            pallas::Base::from(change_coin.value),
            change_coin.rho,
        ];
        let cm3_msg = poseidon_hash(cm3_msg_in);
        let cm3 = pedersen_commitment_base(cm3_msg, change_coin.opening);
        let cm4_msg_in = [
            pallas::Base::from(PREFIX_CM),
            transfered_pk,
            pallas::Base::from(transfered_coin.value),
            transfered_coin.rho,
        ];
        let cm4_msg = poseidon_hash(cm4_msg_in);
        let cm4 = pedersen_commitment_base(cm4_msg, transfered_coin.opening);
        let tx = TransferStx {
            coin_commitment: self.coin1_commitment,
            coin_pk: self.pk(),
            coin_root_sk: self.coin1_sk_root,
            change_coin_commitment: cm3,
            transfered_coin_commitment: cm4,
            nullifier: self.sn(),
            tau: self.tau,
            root: self.coin1_commitment_root,
            proof,
        };
        Ok(tx)
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
