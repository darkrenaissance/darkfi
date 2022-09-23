use halo2_gadgets::poseidon::primitives as poseidon;
use halo2_proofs::arithmetic::Field;
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};

use log::debug;

use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::PrimeField, Curve},
    pallas,
};

use rand::{thread_rng, Rng};

use crate::crypto::{
    constants::MERKLE_DEPTH_ORCHARD,
    lead_proof,
    leadcoin::LeadCoin,
    merkle_node::MerkleNode,
    proof::{Proof, ProvingKey},
    types::DrkValueBlind,
    util::{mod_r_p, pedersen_commitment_base, pedersen_commitment_u64},
};

const PRF_NULLIFIER_PREFIX: u64 = 0;

const MERKLE_DEPTH: u8 = MERKLE_DEPTH_ORCHARD as u8;

#[derive(Copy, Debug, Default, Clone)]
pub struct EpochItem {
    pub value: u64, // the stake value is static during the epoch.
}

/// epoch configuration
/// this struct need be a singleton,
/// should be populated from configuration file.
#[derive(Copy, Debug, Default, Clone)]
pub struct EpochConsensus {
    pub sl_len: u64,
    /// number of slots per epoch
    pub e_len: u64,
    pub tick_len: u64,
    pub reward: u64,
}

impl EpochConsensus {
    pub fn new(
        sl_len: Option<u64>,
        e_len: Option<u64>,
        tick_len: Option<u64>,
        reward: Option<u64>,
    ) -> Self {
        Self {
            sl_len: sl_len.unwrap_or(22),
            e_len: e_len.unwrap_or(3),
            tick_len: tick_len.unwrap_or(22),
            reward: reward.unwrap_or(1),
        }
    }

    /// TODO how is the reward derived?
    pub fn get_reward(&self) -> u64 {
        self.reward
    }

    pub fn get_slot_len(&self) -> u64 {
        self.sl_len
    }

    pub fn get_epoch_len(&self) -> u64 {
        self.e_len
    }

    pub fn get_tick_len(&self) -> u64 {
        self.tick_len
    }
}

#[derive(Debug, Default, Clone)]
pub struct Epoch {
    // TODO this need to emulate epoch
    // should have ep, slot, current block, etc.
    //epoch metadata
    pub len: Option<usize>, // number of slots in the epoch
    //epoch item
    pub item: Option<EpochItem>,
    pub eta: pallas::Base,    // CRS for the leader selection.
    pub coins: Vec<LeadCoin>, // competing coins
}

impl Epoch {
    pub fn new(consensus: EpochConsensus, true_random: pallas::Base) -> Self {
        Self {
            len: Some(consensus.get_slot_len() as usize),
            item: Some(EpochItem { value: consensus.reward }),
            eta: true_random,
            coins: vec![],
        }
    }
    fn create_coins_election_seeds(&self, sl: pallas::Base) -> (pallas::Base, pallas::Base) {
        let election_seed_nonce: pallas::Base = pallas::Base::from(3);
        let election_seed_lead: pallas::Base = pallas::Base::from(22);

        // mu_rho
        let nonce_mu_msg = [election_seed_nonce, self.eta, sl];
        let nonce_mu: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<3>, 3, 2>::init()
                .hash(nonce_mu_msg);
        // mu_y
        let lead_mu_msg = [election_seed_lead, self.eta, sl];
        let lead_mu: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<3>, 3, 2>::init()
                .hash(lead_mu_msg);
        (lead_mu, nonce_mu)
    }

    fn create_coins_sks(&self) -> (Vec<MerkleNode>, Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]>) {
        /*
        at the onset of an epoch, the first slot's coin's secret key
        is sampled at random, and the rest of the secret keys are derived,
        for sk (secret key) at time i+1 is derived from secret key at time i.
         */
        let mut rng = thread_rng();
        let mut tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(self.len.unwrap() as usize);
        let mut root_sks: Vec<MerkleNode> = vec![];
        let mut path_sks: Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]> = vec![];
        let mut prev_sk_base: pallas::Base = pallas::Base::one();
        for _i in 0..self.len.unwrap() {
            let sk_bytes = if _i == 0 {
                let base = pedersen_commitment_u64(1, pallas::Scalar::random(&mut rng));
                let coord = base.to_affine().coordinates().unwrap();
                let sk_base = coord.x() * coord.y();
                prev_sk_base = sk_base;
                sk_base.to_repr()
            } else {
                let base = pedersen_commitment_u64(1, mod_r_p(prev_sk_base));
                let coord = base.to_affine().coordinates().unwrap();
                let sk_base = coord.x() * coord.y();
                prev_sk_base = sk_base;
                sk_base.to_repr()
            };
            let node = MerkleNode::from_bytes(&sk_bytes).unwrap();
            //let serialized = serde_json::to_string(&node).unwrap();
            //debug!("serialized: {}", serialized);
            tree.append(&node.clone());
            let leaf_position = tree.witness();
            let root = tree.root(0).unwrap();
            //let (leaf_pos, path) = tree.authentication_path(leaf_position.unwrap()).unwrap();
            let path = tree.authentication_path(leaf_position.unwrap(), &root).unwrap();
            //note root sk is at tree.root()
            //root_sks.push(node);
            root_sks.push(root);
            path_sks.push(path.as_slice().try_into().unwrap());
        }
        (root_sks, path_sks)
    }
    //note! the strategy here is single competing coin per slot.
    pub fn create_coins(&mut self, sigma: pallas::Base) -> Vec<LeadCoin> {
        let mut rng = thread_rng();
        let mut seeds: Vec<u64> = vec![];
        for _i in 0..self.len.unwrap() {
            let rho: u64 = rng.gen();
            seeds.push(rho);
        }
        let (root_sks, path_sks) = self.create_coins_sks();
        //random commitment blinding values
        let c_cm1_blind: DrkValueBlind = pallas::Scalar::random(&mut rng);
        let c_cm2_blind: DrkValueBlind = pallas::Scalar::random(&mut rng);

        let mut tree_cm = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(self.len.unwrap() as usize);
        let mut coins: Vec<LeadCoin> = vec![];
        for i in 0..self.len.unwrap() {
            let c_v = pallas::Base::from(self.item.unwrap().value);
            //random sampling of the same size of prf,
            //pseudo random sampling that is the size of pederson commitment
            // coin slot number

            let c_sl = pallas::Base::from(u64::try_from(i).unwrap());
            //
            //let's assume it's sl for simplicity
            let c_tau = pallas::Base::from(u64::try_from(i).unwrap());
            //
            let c_root_sk: MerkleNode = root_sks[i];

            let coin_pk_msg = [c_tau, c_root_sk.inner()];
            let c_pk: pallas::Base =
                poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init(
                )
                .hash(coin_pk_msg);

            let c_seed = pallas::Base::from(seeds[i]);
            let sn_msg = [c_seed, c_root_sk.inner()];
            let c_sn: pallas::Base =
                poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init(
                )
                .hash(sn_msg);

            let coin_commit_msg_input =
                [pallas::Base::from(PRF_NULLIFIER_PREFIX), c_pk, c_v, c_seed];
            let coin_commit_msg: pallas::Base =
                poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<4>, 3, 2>::init(
                )
                .hash(coin_commit_msg_input);
            let c_cm: pallas::Point = pedersen_commitment_base(coin_commit_msg, c_cm1_blind);
            let c_cm_coordinates = c_cm.to_affine().coordinates().unwrap();
            let c_cm_base: pallas::Base = c_cm_coordinates.x() * c_cm_coordinates.y();
            let c_cm_node = MerkleNode(c_cm_base);
            tree_cm.append(&c_cm_node.clone());
            let leaf_position = tree_cm.witness();
            let c_root_cm = tree_cm.root(0).unwrap();
            let c_cm_path =
                tree_cm.authentication_path(leaf_position.unwrap(), &c_root_cm).unwrap();

            let coin_nonce2_msg = [c_seed, c_root_sk.inner()];
            let c_seed2: pallas::Base =
                poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init(
                )
                .hash(coin_nonce2_msg);

            let coin2_commit_msg_input =
                [pallas::Base::from(PRF_NULLIFIER_PREFIX), c_pk, c_v, c_seed2];
            let coin2_commit_msg: pallas::Base =
                poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<4>, 3, 2>::init(
                )
                .hash(coin2_commit_msg_input);
            let c_cm2 = pedersen_commitment_base(coin2_commit_msg, c_cm2_blind);

            let c_root_sk = root_sks[i];
            let c_path_sk = path_sks[i];

            // election seeds
            let (y_mu, rho_mu) = self.create_coins_election_seeds(c_sl);
            let coin = LeadCoin {
                value: Some(c_v),
                cm: Some(c_cm),
                cm2: Some(c_cm2),
                idx: u32::try_from(i).unwrap(),
                sl: Some(c_sl),
                tau: Some(c_tau),
                nonce: Some(c_seed),
                nonce_cm: Some(c_seed2),
                sn: Some(c_sn),
                pk: Some(c_pk),
                root_cm: Some(mod_r_p(c_root_cm.inner())),
                root_sk: Some(c_root_sk.inner()),
                path: Some(c_cm_path.as_slice().try_into().unwrap()),
                path_sk: Some(c_path_sk),
                c1_blind: Some(c_cm1_blind),
                c2_blind: Some(c_cm2_blind),
                y_mu: Some(y_mu),
                rho_mu: Some(rho_mu),
                sigma_scalar: Some(sigma),
            };
            coins.push(coin);
        }
        self.coins = coins.clone();
        coins
    }

    /// retrive leadership lottary coins of static stake,
    /// retrived for for commitment in the genesis data
    pub fn get_coins(&self) -> Vec<LeadCoin> {
        self.coins.clone()
    }

    /// see if the participant stakeholder of this epoch is
    /// winning the lottery, in case of success return True
    pub fn is_leader(&self, sl: u64) -> bool {
        let slusize = sl as usize;
        debug!("slot: {}, coin len: {}", sl, self.coins.len());
        assert!(slusize < self.coins.len());
        let coin = self.coins[sl as usize];
        let y_exp = [coin.root_sk.unwrap(), coin.nonce.unwrap()];
        let y_exp_hash: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(y_exp);
        // pick x coordiante of y for comparison
        let y_x: pallas::Base = *pedersen_commitment_base(coin.y_mu.unwrap(), mod_r_p(y_exp_hash))
            .to_affine()
            .coordinates()
            .unwrap()
            .x();
        let ord = pallas::Base::from(10241024); //TODO fine tune this scalar.
        let target = ord * coin.value.unwrap();
        debug!("y_x: {:?}, target: {:?}", y_x, target);
        //reversed for testing
        target < y_x
    }

    pub fn get_proof(&self, sl: u64, pk: &ProvingKey) -> Proof {
        let coin = self.coins[sl as usize];
        lead_proof::create_lead_proof(pk, coin).unwrap()
    }
}

#[derive(Debug, Default, Clone)]
pub struct LifeTime {
    //lifetime metadata
    //...
    //lifetime epochs
    pub epochs: Vec<Epoch>,
}
