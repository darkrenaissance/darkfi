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

use darkfi_sdk::crypto::{constants::MERKLE_DEPTH_ORCHARD, MerkleNode};
use halo2_gadgets::poseidon::primitives as poseidon;
use halo2_proofs::circuit::Value;
use pasta_curves::{arithmetic::CurveAffine, group::Curve, pallas};

use crate::{
    crypto::{
        keypair::Keypair,
        util::{mod_r_p, pedersen_commitment_base},
    },
    zk::circuit::lead_contract::LeadContract,
};

pub const LEAD_PUBLIC_INPUT_LEN: usize = 4;

#[derive(Debug, Default, Clone, Copy)]
pub struct LeadCoin {
    pub value: Option<u64>,             // coin stake
    pub cm: Option<pallas::Point>,      // coin commitment
    pub cm2: Option<pallas::Point>,     // poured coin commitment
    pub idx: u32,                       // coin idex
    pub sl: Option<pallas::Base>,       // coin slot id
    pub tau: Option<pallas::Base>,      // coin time stamp
    pub nonce: Option<pallas::Base>,    // coin nonce
    pub nonce_cm: Option<pallas::Base>, // coin nonce's commitment
    pub sn: Option<pallas::Base>,       // coin's serial number
    pub keypair: Option<Keypair>,
    pub root_cm: Option<pallas::Base>, // root of coin commitment
    pub root_sk: Option<pallas::Base>, // coin's secret key
    pub path: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to the coin's commitment
    pub path_sk: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to the coin's secret key
    pub c1_blind: Option<pallas::Scalar>, // coin opening
    pub c2_blind: Option<pallas::Scalar>, // poured coin opening
    // election seeds
    pub y_mu: Option<pallas::Base>, // leader election nonce derived from eta at onset of epoch
    pub rho_mu: Option<pallas::Base>, // leader election nonce derived from eta at onset of epoch
    pub sigma1: Option<pallas::Base>,
    pub sigma2: Option<pallas::Base>,
}

impl LeadCoin {
    pub fn public_inputs_as_array(&self) -> [pallas::Base; LEAD_PUBLIC_INPUT_LEN] {
        let po_nonce = self.nonce_cm.unwrap();
        let po_pk = self.keypair.unwrap().public.0.to_affine().coordinates().unwrap();
        let y_mu = self.y_mu.unwrap();
        let rho_mu = self.rho_mu.unwrap();
        let root_sk = self.root_sk.unwrap();
        let nonce = self.nonce.unwrap();
        let lottery_msg_input = [root_sk, nonce];
        let lottery_msg: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(lottery_msg_input);
        let po_y_pt: pallas::Point = pedersen_commitment_base(lottery_msg, mod_r_p(y_mu));
        let po_y_x = *po_y_pt.to_affine().coordinates().unwrap().x();
        let po_y_y = *po_y_pt.to_affine().coordinates().unwrap().y();
        let y_coord_arr = [po_y_x, po_y_y];
        let po_y: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(y_coord_arr);
        let cm_pos = self.idx;
        let public_inputs: [pallas::Base; LEAD_PUBLIC_INPUT_LEN] =
            [po_nonce, *po_pk.x(), *po_pk.y(), po_y];
        public_inputs
    }

    pub fn public_inputs(&self) -> Vec<pallas::Base> {
        self.public_inputs_as_array().to_vec()
    }

    pub fn create_contract(&self) -> LeadContract {
        let rho_mu = self.rho_mu.unwrap();
        let root_sk = self.root_sk.unwrap();
        let nonce = self.nonce.unwrap();
        let lottery_msg_input = [root_sk, nonce];
        let lottery_msg: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(lottery_msg_input);
        //
        let rho_pt: pallas::Point = pedersen_commitment_base(lottery_msg, mod_r_p(rho_mu));
        LeadContract {
            coin1_commit_merkle_path: Value::known(self.path.unwrap()),
            coin1_commit_root: Value::known(self.root_cm.unwrap()),
            coin1_commit_leaf_pos: Value::known(self.idx),
            coin1_sk: Value::known(self.keypair.unwrap().secret.inner()),
            coin1_sk_root: Value::known(self.root_sk.unwrap()),
            coin1_sk_merkle_path: Value::known(self.path_sk.unwrap()),
            coin1_timestamp: Value::known(self.tau.unwrap()), //
            coin1_nonce: Value::known(self.nonce.unwrap()),
            coin1_blind: Value::known(self.c1_blind.unwrap()),
            coin1_serial: Value::known(self.sn.unwrap()),
            coin1_value: Value::known(pallas::Base::from(self.value.unwrap())),
            coin2_blind: Value::known(self.c2_blind.unwrap()),
            coin2_commit: Value::known(self.cm2.unwrap()),
            mau_rho: Value::known(mod_r_p(self.rho_mu.unwrap())),
            mau_y: Value::known(mod_r_p(self.y_mu.unwrap())),
            sigma1: Value::known(self.sigma1.unwrap()),
            sigma2: Value::known(self.sigma2.unwrap()),
            rho: Value::known(rho_pt),
        }
    }
}
