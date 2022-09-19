use pasta_curves::pallas;
use halo2_proofs::{circuit::Value};
use halo2_gadgets::{
    poseidon::{primitives as poseidon},
};

use crate::{
    zk::circuit::lead_contract::LeadContract,
    crypto::{
        constants::MERKLE_DEPTH_ORCHARD,
        merkle_node::MerkleNode,
        util::{mod_r_p, pedersen_commitment_base},
    }
};

use incrementalmerkletree::Hashable;

use pasta_curves::{arithmetic::CurveAffine, group::Curve};

//use halo2_proofs::arithmetic::CurveAffine;

pub const LEAD_PUBLIC_INPUT_LEN : usize = 10;

#[derive(Debug, Default, Clone, Copy)]
pub struct LeadCoin {
    pub value: Option<pallas::Base>, // coin stake
    pub cm: Option<pallas::Point>, // coin commitment
    pub cm2: Option<pallas::Point>, // poured coin commitment
    pub idx: u32, // coin idex
    pub sl: Option<pallas::Base>, // coin slot id
    pub tau: Option<pallas::Base>, // coin time stamp
    pub nonce: Option<pallas::Base>, // coin nonce
    pub nonce_cm: Option<pallas::Base>, // coin nonce's commitment
    pub sn: Option<pallas::Base>, // coin's serial number
    pub pk: Option<pallas::Base>, // coin public key
    pub root_cm: Option<pallas::Scalar>, // root of coin commitment
    pub root_sk: Option<pallas::Base>, // coin's secret key
    pub path: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to the coin's commitment
    pub path_sk: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>, // path to the coin's secret key
    pub c1_blind: Option<pallas::Scalar>, // coin opening
    pub c2_blind: Option<pallas::Scalar>, // poured coin opening
    // election seeds
    pub y_mu: Option<pallas::Base>, // leader election nonce derived from eta at onset of epoch
    pub rho_mu: Option<pallas::Base>, // leader election nonce derived from eta at onset of epoch
    pub sigma_scalar: Option<pallas::Base>,
}

impl LeadCoin {
    pub fn public_inputs_as_array(&self) -> [pallas::Base;LEAD_PUBLIC_INPUT_LEN] {
        let po_nonce = self.nonce_cm.unwrap();
        let _po_tau = pedersen_commitment_base(self.tau.unwrap(), self.root_cm.unwrap())
            .to_affine()
            .coordinates()
            .unwrap();

        let po_cm = self.cm.unwrap().to_affine().coordinates().unwrap();
        let po_cm2 = self.cm2.unwrap().to_affine().coordinates().unwrap();
        let po_pk = self.pk.unwrap();
        let po_sn = self.sn.unwrap();

        let y_mu = self.y_mu.unwrap();
        let rho_mu = self.rho_mu.unwrap();
        let root_sk  = self.root_sk.unwrap();
        let nonce = self.nonce.unwrap();
        let lottery_msg_input = [
            root_sk,
            nonce,
        ];
        let lottery_msg : pallas::Base = poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init().hash(lottery_msg_input);
        //
        let po_y_pt: pallas::Point = pedersen_commitment_base(lottery_msg, mod_r_p(y_mu));
        let po_y = *po_y_pt.to_affine().coordinates().unwrap().x();
        //
        let po_rho_pt: pallas::Point = pedersen_commitment_base(lottery_msg, mod_r_p(rho_mu));
        let po_rho = *po_rho_pt.to_affine().coordinates().unwrap().x();


        let _zero = pallas::Base::from(0);

        // ===============

        let cm_pos = self.idx;
        let cm_root = {
            let pos: u32 = cm_pos;
            let c_cm_coordinates = self.cm.unwrap().to_affine().coordinates().unwrap();
            let c_cm_base: pallas::Base = c_cm_coordinates.x() * c_cm_coordinates.y();
            let mut current = MerkleNode(c_cm_base);
            for (level, sibling) in self.path.unwrap().iter().enumerate() {
                let level = level as u8;
                current = if pos & (1 << level) == 0 {
                    MerkleNode::combine(level.into(), &current, sibling)
                } else {
                    MerkleNode::combine(level.into(), sibling, &current)
                };
            }
            current
        };
        let public_inputs : [pallas::Base;LEAD_PUBLIC_INPUT_LEN] = [

            *po_cm.x(),
            *po_cm.y(),

            *po_cm2.x(),
            *po_cm2.y(),

            po_nonce,
            cm_root.0,

            po_pk,
            po_sn,

            po_y,
            po_rho,
        ];
        public_inputs
    }

    pub fn public_inputs(&self) -> Vec<pallas::Base> {
        self.public_inputs_as_array().to_vec()
    }

    pub fn create_contract(&self) -> LeadContract {
        let contract = LeadContract {
            path: Value::known(self.path.unwrap()),
            root_sk: Value::known(self.root_sk.unwrap()),
            path_sk: Value::known(self.path_sk.unwrap()),
            coin_timestamp: Value::known(self.tau.unwrap()), //
            coin_nonce: Value::known(self.nonce.unwrap()),
            coin1_blind: Value::known(self.c1_blind.unwrap()),
            value: Value::known(self.value.unwrap()),
            coin2_blind: Value::known(self.c2_blind.unwrap()),
            cm_pos: Value::known(self.idx),
            //sn_c1: Value::known(self.sn.unwrap()),
            slot: Value::known(self.sl.unwrap()),
            mau_rho: Value::known(mod_r_p(self.rho_mu.unwrap())),
            mau_y: Value::known(mod_r_p(self.y_mu.unwrap())),
            root_cm: Value::known(self.root_cm.unwrap()),
            sigma_scalar: Value::known(self.sigma_scalar.unwrap()),
        };
        contract
    }
}
