use darkfi_sdk::crypto::{constants::MERKLE_DEPTH_ORCHARD, MerkleNode};
use halo2_gadgets::poseidon::primitives as poseidon;
use halo2_proofs::circuit::Value;
use incrementalmerkletree::Hashable;
use pasta_curves::{arithmetic::CurveAffine, group::Curve, pallas};

use crate::{
    crypto::{
        keypair::Keypair,
        util::{mod_r_p, pedersen_commitment_base},
    },
    zk::circuit::lead_contract::LeadContract,
};

pub const LEAD_PUBLIC_INPUT_LEN: usize = 7;

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
    pub root_cm: Option<pallas::Scalar>, // root of coin commitment
    pub root_sk: Option<pallas::Base>,   // coin's secret key
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
        let _po_tau = pedersen_commitment_base(self.tau.unwrap(), self.root_cm.unwrap())
            .to_affine()
            .coordinates()
            .unwrap();

        let po_cm = self.cm.unwrap().to_affine().coordinates().unwrap();
        let po_pk = self.keypair.unwrap().public.0.to_affine().coordinates().unwrap();

        let y_mu = self.y_mu.unwrap();
        let rho_mu = self.rho_mu.unwrap();
        let root_sk = self.root_sk.unwrap();
        let nonce = self.nonce.unwrap();
        let lottery_msg_input = [root_sk, nonce];
        let lottery_msg: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(lottery_msg_input);
        //
        let po_y_pt: pallas::Point = pedersen_commitment_base(lottery_msg, mod_r_p(y_mu));
        let po_y_x = *po_y_pt.to_affine().coordinates().unwrap().x();
        let po_y_y = *po_y_pt.to_affine().coordinates().unwrap().y();
        let y_coord_arr = [po_y_x, po_y_y];
        let po_y: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(y_coord_arr);

        let cm_pos = self.idx;
        let cm_root = {
            let pos: u32 = cm_pos;
            let c_cm_coordinates = self.cm.unwrap().to_affine().coordinates().unwrap();
            let c_cm_base: pallas::Base = c_cm_coordinates.x() * c_cm_coordinates.y();
            let mut current = MerkleNode::from(c_cm_base);
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
        let public_inputs: [pallas::Base; LEAD_PUBLIC_INPUT_LEN] =
            [*po_cm.x(), *po_cm.y(), po_nonce, cm_root.inner(), *po_pk.x(), *po_pk.y(), po_y];
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
            path: Value::known(self.path.unwrap()),
            sk: Value::known(self.keypair.unwrap().secret.inner()),
            root_sk: Value::known(self.root_sk.unwrap()),
            path_sk: Value::known(self.path_sk.unwrap()),
            coin_timestamp: Value::known(self.tau.unwrap()), //
            coin_nonce: Value::known(self.nonce.unwrap()),
            coin1_blind: Value::known(self.c1_blind.unwrap()),
            coin1_sn: Value::known(self.sn.unwrap()),
            value: Value::known(pallas::Base::from(self.value.unwrap())),
            coin2_blind: Value::known(self.c2_blind.unwrap()),
            coin2_commit: Value::known(self.cm2.unwrap()),
            cm_pos: Value::known(self.idx),
            //sn_c1: Value::known(self.sn.unwrap()),
            slot: Value::known(self.sl.unwrap()),
            mau_rho: Value::known(mod_r_p(self.rho_mu.unwrap())),
            mau_y: Value::known(mod_r_p(self.y_mu.unwrap())),
            root_cm: Value::known(self.root_cm.unwrap()),
            sigma1: Value::known(self.sigma1.unwrap()),
            sigma2: Value::known(self.sigma2.unwrap()),
            rho: Value::known(rho_pt),
        }
    }
}
