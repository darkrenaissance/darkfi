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

use darkfi_sdk::{
    crypto::{
        pedersen::{pedersen_commitment_base, pedersen_commitment_u64},
        poseidon_hash,
        util::mod_r_p,
        MerkleNode, SecretKey,
    },
    incrementalmerkletree::{bridgetree::BridgeTree, Tree},
    pasta::{arithmetic::CurveAffine, group::Curve, pallas},
};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use halo2_proofs::{arithmetic::Field, circuit::Value};
use log::info;
use rand::rngs::OsRng;

use super::constants::EPOCH_LENGTH;
use crate::{
    consensus::{constants, utils::fbig2base, Float10, TransferStx, TxRcpt},
    zk::{
        proof::{Proof, ProvingKey},
        vm::ZkCircuit,
        vm_stack::Witness,
    },
    zkas::ZkBinary,
    Result,
};

use std::{
    fs::File,
    io::{prelude::*, BufWriter},
};

pub const MERKLE_DEPTH_LEAD_COIN: usize = 32;
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
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct LeadCoin {
    /// Coin's stake value
    pub value: u64,
    /// Coin creation slot.
    pub slot: u64,
    /// Coin nonce
    pub nonce: pallas::Base,
    /// Commitment for coin1
    pub coin1_commitment: pallas::Point,
    /// Merkle root of coin1 commitment
    pub coin1_commitment_root: MerkleNode,
    /// Coin commitment position
    pub coin1_commitment_pos: u32,
    /// Merkle path to the coin1's commitment
    pub coin1_commitment_merkle_path: Vec<MerkleNode>,
    /// coin1 sk
    pub coin1_sk: pallas::Base,
    /// Merkle root of the `coin1` secret key
    pub coin1_sk_root: MerkleNode,
    /// coin1 sk position in merkle tree
    pub coin1_sk_pos: u32,
    /// Merkle path to the secret key of `coin1`
    pub coin1_sk_merkle_path: Vec<MerkleNode>,
    /// coin1 commitment blinding factor
    pub coin1_blind: pallas::Scalar,
}

impl LeadCoin {
    /// Create a new `LeadCoin` object using given parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        // emulation of global random oracle output from previous epoch randomness.
        //eta: pallas::Base,
        // Stake value
        value: u64,
        // Slot absolute index
        slot: u64,
        // coin1 sk
        coin1_sk: pallas::Base,
        // Merkle root of the `coin_1` secret key in the Merkle tree of secret keys
        coin1_sk_root: MerkleNode,
        // sk pos
        coin1_sk_pos: usize,
        // Merkle path to the secret key of `coin_1` in the Merkle tree of secret keys
        coin1_sk_merkle_path: Vec<MerkleNode>,
        // coin1 nonce
        seed: pallas::Base,
        // Merkle tree of coin commitments
        coin_commitment_tree: &mut BridgeTree<MerkleNode, MERKLE_DEPTH>,
    ) -> Self {
        // Generate random blinding values for commitments:
        let coin1_blind = pallas::Scalar::random(&mut OsRng);
        //let coin2_blind = pallas::Scalar::random(&mut OsRng);
        // pk
        let pk = Self::util_pk(coin1_sk_root, slot);
        let coin1_commitment = Self::commitment(pk, pallas::Base::from(value), seed, coin1_blind);
        // Hash its coordinates to get a base field element
        let c1_cm_coords = coin1_commitment.to_affine().coordinates().unwrap();
        let c1_base_msg = [*c1_cm_coords.x(), *c1_cm_coords.y()];
        let coin1_commitment_base = poseidon_hash(c1_base_msg);
        // Append the element to the Merkle tree
        coin_commitment_tree.append(&MerkleNode::from(coin1_commitment_base));
        let coin1_commitment_pos = coin_commitment_tree.witness().unwrap();
        let coin1_commitment_root = coin_commitment_tree.root(0).unwrap();
        let coin1_commitment_merkle_path = coin_commitment_tree
            .authentication_path(coin1_commitment_pos, &coin1_commitment_root)
            .unwrap();

        Self {
            value,
            slot,
            nonce: seed,
            coin1_commitment,
            coin1_commitment_root,
            coin1_commitment_pos: u32::try_from(usize::from(coin1_commitment_pos)).unwrap(),
            coin1_commitment_merkle_path,
            coin1_sk,
            coin1_sk_root,
            coin1_sk_pos: u32::try_from(coin1_sk_pos).unwrap(),
            coin1_sk_merkle_path,
            coin1_blind,
        }
    }

    pub fn sn(&self) -> pallas::Base {
        let sn_msg = [pallas::Base::from(PREFIX_SN), self.coin1_sk_root.inner(), self.nonce, ZERO];
        poseidon_hash(sn_msg)
    }

    pub fn election_seeds_u64(eta: pallas::Base, slotu64: u64) -> (pallas::Base, pallas::Base) {
        Self::election_seeds(eta, pallas::Base::from(slotu64))
    }

    /// Derive election seeds from given parameters
    pub fn election_seeds(eta: pallas::Base, slot: pallas::Base) -> (pallas::Base, pallas::Base) {
        info!(target: "consensus::leadcoin", "election_seeds: eta: {:?}, slot: {:?}", eta, slot);
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
    pub fn public_inputs(
        &self,
        sigma1: pallas::Base,
        sigma2: pallas::Base,
        current_eta: pallas::Base,
        current_slot: pallas::Base,
        derived_blind: pallas::Scalar,
    ) -> Vec<pallas::Base> {
        // pk
        let pk = self.pk();
        // coin 1-2 cm/commitment
        let c1_cm_coord = self.coin1_commitment.to_affine().coordinates().unwrap();
        let c2_cm_coord = self.derived_commitment(derived_blind).to_affine().coordinates().unwrap();
        // lottery seed
        let seed_msg =
            [pallas::Base::from(PREFIX_SEED), self.coin1_sk_root.inner(), self.nonce, ZERO];
        let seed = poseidon_hash(seed_msg);
        // y
        let (y_mu, rho_mu) = Self::election_seeds(current_eta, current_slot);
        let y_msg = [seed, y_mu];
        let y = poseidon_hash(y_msg);
        // rho
        let rho_msg = [seed, rho_mu];
        let rho = poseidon_hash(rho_msg);
        let public_inputs = vec![
            pk,
            *c1_cm_coord.x(),
            *c1_cm_coord.y(),
            *c2_cm_coord.x(),
            *c2_cm_coord.y(),
            self.coin1_commitment_root.inner(),
            self.coin1_sk_root.inner(),
            self.sn(),
            y_mu,
            y,
            rho_mu,
            rho,
            sigma1,
            sigma2,
        ];
        public_inputs
    }

    fn util_pk(sk_root: MerkleNode, slot: u64) -> pallas::Base {
        let pk_msg =
            [pallas::Base::from(PREFIX_PK), sk_root.inner(), pallas::Base::from(slot), ZERO];

        poseidon_hash(pk_msg)
    }
    /// calculate coin public key: hash of root coin secret key
    /// and creation slot.
    pub fn pk(&self) -> pallas::Base {
        Self::util_pk(self.coin1_sk_root, self.slot)
    }

    fn util_derived_rho(sk_root: MerkleNode, nonce: pallas::Base) -> pallas::Base {
        let rho_msg = [pallas::Base::from(PREFIX_EVL), sk_root.inner(), nonce, ZERO];

        poseidon_hash(rho_msg)
    }
    /// calculate derived coin nonce: hash of root coin secret key
    /// and old nonce
    pub fn derived_rho(&self) -> pallas::Base {
        Self::util_derived_rho(self.coin1_sk_root, self.nonce)
    }

    pub fn headstart() -> pallas::Base {
        let headstart = constants::MIN_F.clone() * Float10::try_from(constants::P.clone()).unwrap();
        let headstart_base = fbig2base(headstart);
        headstart_base
    }

    pub fn is_leader(
        &self,
        sigma1: pallas::Base,
        sigma2: pallas::Base,
        current_eta: pallas::Base,
        current_slot: pallas::Base,
    ) -> bool {
        let y_seed =
            [pallas::Base::from(PREFIX_SEED), self.coin1_sk_root.inner(), self.nonce, ZERO];
        let y_seed_hash = poseidon_hash(y_seed);
        let (y_mu, _) = Self::election_seeds(current_eta, current_slot);
        let y_msg = [y_seed_hash, y_mu];
        let y = poseidon_hash(y_msg);

        let value = pallas::Base::from(self.value);

        let headstart = Self::headstart();
        let target = sigma1 * value + sigma2 * value * value + headstart;

        let y_t_str = format!("{:?},{:?}\n", y, target);
        let f =
            File::options().append(true).create(true).open(constants::LOTTERY_HISTORY_LOG).unwrap();

        {
            let mut writer = BufWriter::new(f);
            writer.write(&y_t_str.into_bytes()).unwrap();
        }
        info!(target: "consensus::leadcoin", "is_leader(): y = {:?}", y);
        info!(target: "consensus::leadcoin", "is_leader(): T = {:?}", target);

        y < target
    }

    fn commitment(
        pk: pallas::Base,
        value: pallas::Base,
        seed: pallas::Base,
        blind: pallas::Scalar,
    ) -> pallas::Point {
        let commit_msg = [pallas::Base::from(PREFIX_CM), pk, value, seed];
        // Create commitment to coin
        let commit_v = poseidon_hash(commit_msg);
        pedersen_commitment_base(commit_v, blind)
    }
    /// calculated derived coin commitment
    pub fn derived_commitment(&self, blind: pallas::Scalar) -> pallas::Point {
        let pk = self.pk();
        let rho = self.derived_rho();
        Self::commitment(pk, pallas::Base::from(self.value + constants::REWARD), rho, blind)
    }

    /// the new coin to be minted after the current coin is spent
    /// in lottery.
    pub fn derive_coin(
        &self,
        coin_commitment_tree: &mut BridgeTree<MerkleNode, MERKLE_DEPTH>,
        derived_blind: pallas::Scalar,
    ) -> LeadCoin {
        info!(target: "consensus::leadcoin", "derive_coin(): Deriving new coin!");
        let derived_c1_rho = self.derived_rho();
        let derived_c1_cm = self.derived_commitment(derived_blind);
        let derived_c1_cm_coord = derived_c1_cm.to_affine().coordinates().unwrap();
        let derived_c1_cm_msg = [*derived_c1_cm_coord.x(), *derived_c1_cm_coord.y()];
        let derived_c1_cm_base = poseidon_hash(derived_c1_cm_msg);
        coin_commitment_tree.append(&MerkleNode::from(derived_c1_cm_base));
        let leaf_pos = coin_commitment_tree.witness().unwrap();
        let commitment_root = coin_commitment_tree.root(0).unwrap();
        let commitment_merkle_path =
            coin_commitment_tree.authentication_path(leaf_pos, &commitment_root).unwrap();
        LeadCoin {
            value: self.value + constants::REWARD,
            slot: self.slot,
            nonce: derived_c1_rho,
            coin1_commitment: derived_c1_cm,
            coin1_commitment_root: commitment_root,
            coin1_commitment_pos: u32::try_from(usize::from(leaf_pos)).unwrap(),
            coin1_commitment_merkle_path: commitment_merkle_path.try_into().unwrap(),
            coin1_sk: self.coin1_sk,
            coin1_sk_root: self.coin1_sk_root,
            coin1_sk_pos: self.coin1_sk_pos,
            coin1_sk_merkle_path: self.coin1_sk_merkle_path.clone(),
            coin1_blind: derived_blind,
        }
    }

    /// Try to create a ZK proof of consensus leadership
    pub fn create_lead_proof(
        &self,
        sigma1: pallas::Base,
        sigma2: pallas::Base,
        eta: pallas::Base,
        slot: pallas::Base, //current slot index.
        pk: &ProvingKey,
        derived_blind: pallas::Scalar,
    ) -> (Result<Proof>, Vec<pallas::Base>) {
        let (y_mu, rho_mu) = Self::election_seeds(eta, slot);
        let bincode = include_bytes!("../../proof/lead.zk.bin");
        let zkbin = ZkBinary::decode(bincode).unwrap();
        let headstart = Self::headstart();
        let coin1_commitment_merkle_path: [MerkleNode; MERKLE_DEPTH_LEAD_COIN] =
            self.coin1_commitment_merkle_path.clone().try_into().unwrap();
        let coin1_sk_merkle_path: [MerkleNode; MERKLE_DEPTH_LEAD_COIN] =
            self.coin1_sk_merkle_path.clone().try_into().unwrap();
        let witnesses = vec![
            Witness::MerklePath(Value::known(coin1_commitment_merkle_path)),
            Witness::Uint32(Value::known(self.coin1_commitment_pos)),
            Witness::Uint32(Value::known(self.coin1_sk_pos)),
            Witness::Base(Value::known(self.coin1_sk)),
            Witness::Base(Value::known(self.coin1_sk_root.inner())),
            Witness::MerklePath(Value::known(coin1_sk_merkle_path)),
            Witness::Base(Value::known(pallas::Base::from(self.slot))),
            Witness::Base(Value::known(self.nonce)),
            Witness::Scalar(Value::known(self.coin1_blind)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Scalar(Value::known(derived_blind)),
            Witness::Base(Value::known(rho_mu)),
            Witness::Base(Value::known(y_mu)),
            Witness::Base(Value::known(sigma1)),
            Witness::Base(Value::known(sigma2)),
            Witness::Base(Value::known(headstart)),
        ];
        let circuit = ZkCircuit::new(witnesses, zkbin);
        let public_inputs = self.public_inputs(sigma1, sigma2, eta, slot, derived_blind);
        (Ok(Proof::create(pk, &[circuit], &public_inputs, &mut OsRng).unwrap()), public_inputs)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_xfer_proof(
        &self,
        pk: &ProvingKey,
        change_coin: TxRcpt,
        change_pk: pallas::Base, //change coin public key
        transfered_coin: TxRcpt,
        transfered_pk: pallas::Base, // recipient coin's public key
        sigma1: pallas::Base,
        sigma2: pallas::Base,
        current_eta: pallas::Base,
        current_slot: pallas::Base,
        derived_blind: pallas::Scalar,
    ) -> Result<TransferStx> {
        assert!(change_coin.value + transfered_coin.value == self.value && self.value > 0);
        let bincode = include_bytes!("../../proof/tx.zk.bin");
        let zkbin = ZkBinary::decode(bincode)?;
        let retval = pallas::Base::from(change_coin.value);
        let xferval = pallas::Base::from(transfered_coin.value);
        let pos: u32 = self.coin1_commitment_pos;
        let value = pallas::Base::from(self.value);
        let coin1_sk_merkle_path: [MerkleNode; MERKLE_DEPTH_LEAD_COIN] =
            self.coin1_sk_merkle_path.clone().try_into().unwrap();
        let coin1_commitment_merkle_path: [MerkleNode; MERKLE_DEPTH_LEAD_COIN] =
            self.coin1_commitment_merkle_path.clone().try_into().unwrap();
        let witnesses = vec![
            // coin (1) burned coin
            Witness::Base(Value::known(self.coin1_commitment_root.inner())),
            Witness::Base(Value::known(self.coin1_sk_root.inner())),
            Witness::Base(Value::known(self.coin1_sk)),
            Witness::MerklePath(Value::known(coin1_sk_merkle_path)),
            Witness::Uint32(Value::known(self.coin1_sk_pos)),
            Witness::Base(Value::known(self.nonce)),
            Witness::Scalar(Value::known(self.coin1_blind)),
            Witness::Base(Value::known(value)),
            Witness::MerklePath(Value::known(coin1_commitment_merkle_path)),
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
        let circuit = ZkCircuit::new(witnesses, zkbin);
        let proof = Proof::create(
            pk,
            &[circuit],
            &self.public_inputs(sigma1, sigma2, current_eta, current_slot, derived_blind),
            &mut OsRng,
        )?;
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
            slot: pallas::Base::from(self.slot),
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
    pub merkle_paths: Vec<Vec<MerkleNode>>,
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
            path_sks.push(path);
        }

        Self { secret_keys: sks, merkle_roots: root_sks, merkle_paths: path_sks }
    }
}
