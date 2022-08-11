use pasta_curves::pallas;
use halo2_proofs::{circuit::Value};
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

#[derive(Debug, Default, Clone, Copy)]
pub struct LeadCoin {
    pub value: Option<pallas::Base>, //stake
    pub cm: Option<pallas::Point>,
    pub cm2: Option<pallas::Point>,
    pub idx: u32,
    pub sl: Option<pallas::Base>, //slot id
    pub tau: Option<pallas::Base>,
    pub nonce: Option<pallas::Base>,
    pub nonce_cm: Option<pallas::Base>,
    pub sn: Option<pallas::Base>, // coin's serial number
    //sk : Option<SecretKey>,
    pub pk: Option<pallas::Base>,
    pub root_cm: Option<pallas::Scalar>,
    pub root_sk: Option<pallas::Base>,
    pub path: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    pub path_sk: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    pub c1_blind: Option<pallas::Scalar>,
    pub c2_blind: Option<pallas::Scalar>,
    // election seeds
    pub y_mu: Option<pallas::Base>,
    pub rho_mu: Option<pallas::Base>,
}

impl LeadCoin {
    pub fn public_inputs(&self) -> Vec<pallas::Base> {
        let po_nonce = self.nonce_cm.unwrap();
        let _po_tau = pedersen_commitment_base(self.tau.unwrap(), self.root_cm.unwrap())
            .to_affine()
            .coordinates()
            .unwrap();

        let po_cm = self.cm.unwrap().to_affine().coordinates().unwrap();
        let po_cm2 = self.cm2.unwrap().to_affine().coordinates().unwrap();

        let po_pk = self.pk.unwrap();
        let po_sn = self.sn.unwrap();

        let po_cmp = pallas::Base::from(1);
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
        let public_inputs: Vec<pallas::Base> = vec![
            po_pk,
            po_sn,
            *po_cm.x(),
            *po_cm.y(),
            *po_cm2.x(),
            *po_cm2.y(),
            po_nonce,
            //cm_root.0,
        ];
        public_inputs
    }

    pub fn create_contract(&self) -> LeadContract
    {
        let contract = LeadContract {
            path: Value::known(self.path.unwrap()),
            coin_pk: Value::known(self.pk.unwrap()),
            root_sk: Value::known(self.root_sk.unwrap()),
            sf_root_sk: Value::known(mod_r_p(self.root_sk.unwrap())),
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
        };
        contract
    }
}
