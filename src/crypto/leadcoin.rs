use pasta_curves::pallas;

use crate::crypto::{
    constants::MERKLE_DEPTH_ORCHARD,
    merkle_node::MerkleNode,
    util::{mod_r_p, pedersen_commitment_scalar},
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
    pub nonce_cm: Option<pallas::Point>,
    pub sn: Option<pallas::Point>, // coin's serial number
    //sk : Option<SecretKey>,
    pub pk: Option<pallas::Point>,
    pub pk_x: Option<pallas::Base>,
    pub pk_y: Option<pallas::Base>,
    pub root_cm: Option<pallas::Scalar>,
    pub root_sk: Option<pallas::Base>,
    pub path: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    pub path_sk: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    pub c1_blind: Option<pallas::Scalar>,
    pub c2_blind: Option<pallas::Scalar>,
}

impl LeadCoin {
    pub fn public_inputs(&self) -> Vec<pallas::Base> {
        let po_nonce = self.nonce_cm.unwrap().to_affine().coordinates().unwrap();

        let _po_tau = pedersen_commitment_scalar(mod_r_p(self.tau.unwrap()), self.root_cm.unwrap())
            .to_affine()
            .coordinates()
            .unwrap();

        let po_cm = self.cm.unwrap().to_affine().coordinates().unwrap();
        let po_cm2 = self.cm2.unwrap().to_affine().coordinates().unwrap();

        let po_pk = self.pk.unwrap().to_affine().coordinates().unwrap();
        let po_sn = self.sn.unwrap().to_affine().coordinates().unwrap();

        let po_cmp = pallas::Base::from(0);
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
            *po_nonce.x(),
            *po_nonce.y(),
            *po_pk.x(),
            *po_pk.y(),
            *po_sn.x(),
            *po_sn.y(),
            *po_cm.x(),
            *po_cm.y(),
            //*po_cm2.x(),
            //*po_cm2.y(),
            //cm_root.0,
            //po_cmp,
        ];
        public_inputs
    }
}
