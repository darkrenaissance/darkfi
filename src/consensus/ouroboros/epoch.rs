use darkfi_sdk::crypto::{constants::MERKLE_DEPTH_ORCHARD, MerkleNode};
use halo2_gadgets::poseidon::primitives as poseidon;
use halo2_proofs::arithmetic::Field;
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use log::info;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::PrimeField, Curve},
    pallas,
};
use rand::{thread_rng, Rng};
use crate::{
    consensus::ouroboros::{
        consts::{RADIX_BITS, LOTTERY_HEAD_START},
        utils::{base2ibig, fbig2ibig},
        EpochConsensus, Float10,
    },
    crypto::{
        coin::OwnCoin,
        keypair::{Keypair, SecretKey},
        lead_proof,
        leadcoin::LeadCoin,
        proof::{Proof, ProvingKey},
        types::DrkValueBlind,
        util::{mod_r_p, pedersen_commitment_base, pedersen_commitment_u64},
    },
};

const PRF_NULLIFIER_PREFIX: u64 = 0;
const MERKLE_DEPTH: u8 = MERKLE_DEPTH_ORCHARD as u8;

#[derive(Debug, Default, Clone)]
pub struct Epoch {
    pub consensus: EpochConsensus,
    // should have ep, slot, current block, etc.
    pub eta: pallas::Base,     // CRS for the leader selection.
    coins: Vec<Vec<LeadCoin>>, // competing coins
}

impl Epoch {
    pub fn new(consensus: EpochConsensus, true_random: pallas::Base) -> Self {
        Self { consensus, eta: true_random, coins: vec![] }
    }

    /// retrive leadership lottary coins of static stake,
    /// retrived for for commitment in the genesis data
    pub fn get_coins(&self) -> Vec<Vec<LeadCoin>> {
        self.coins.clone()
    }

    pub fn get_coin(&self, sl: usize, idx: usize) -> LeadCoin {
        self.coins[sl][idx]
    }

    pub fn len(&self) -> usize {
        self.consensus.get_epoch_len() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn col(&self) -> usize {
        if self.coins.is_empty() {
            0
        } else {
            self.coins[0].len()
        }
    }

    //
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

    /// at the onset of an epoch, the first slot's coin's secret key
    /// is sampled at random, and the rest of the secret keys are derived,
    /// for sk (secret key) at time i+1 is derived from secret key at time i.
    ///
    fn create_coins_sks(
        &self,
        sks: &mut Vec<SecretKey>,
    ) -> (Vec<MerkleNode>, Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]>) {
        let mut rng = thread_rng();
        let mut tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(self.len());
        let mut root_sks: Vec<MerkleNode> = vec![];
        let mut path_sks: Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]> = vec![];
        let mut prev_sk_base: pallas::Base = pallas::Base::one();
        for _i in 0..self.len() {
            //TODO (fix) add sk for the coin struct to be used in txs decryption of tx notes.
            let base: pallas::Point = if _i == 0 {
                pedersen_commitment_u64(1, pallas::Scalar::random(&mut rng))
            } else {
                pedersen_commitment_u64(1, mod_r_p(prev_sk_base))
            };
            let coord = base.to_affine().coordinates().unwrap();
            //TODO (fix) change this to sk = hash(x,y)
            let sk_base = coord.x() * coord.y();
            sks.push(SecretKey::from(sk_base));
            prev_sk_base = sk_base;
            let sk_bytes = sk_base.to_repr();
            let node = MerkleNode::from_bytes(sk_bytes).unwrap();
            //let serialized = serde_json::to_string(&node).unwrap();
            //info!("serialized: {}", serialized);
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
    pub fn create_coins(
        &mut self,
        sigma1: pallas::Base,
        sigma2: pallas::Base,
        owned: Vec<OwnCoin>,
    ) -> Vec<Vec<LeadCoin>> {
        let mut rng = thread_rng();
        let mut seeds: Vec<u64> = vec![];
        for _i in 0..self.len() {
            let rho: u64 = rng.gen();
            seeds.push(rho);
        }
        let mut sks: Vec<SecretKey> = vec![];
        let (root_sks, path_sks) = self.create_coins_sks(&mut sks);

        // matrix of leadcoins, each row has competing coins per slot.
        let _coins: Vec<Vec<LeadCoin>> = vec![];
        for i in 0..self.len() {
            // if you have any stake used is for competition
            if !owned.is_empty() {
                let mut slot_coins = vec![];
                for elem in &owned {
                    let coin = self.create_leadcoin(
                        sigma1,
                        sigma2,
                        elem.note.value,
                        i,
                        root_sks[i],
                        path_sks[i],
                        seeds[i],
                        sks[i],
                    );
                    slot_coins.push(coin);
                }
                self.coins.push(slot_coins);
            }
            // otherwise compete with zero stake
            else {
                let coin = self.create_leadcoin(
                    sigma1,
                    sigma2,
                    LOTTERY_HEAD_START,
                    i,
                    root_sks[i],
                    path_sks[i],
                    seeds[i],
                    sks[i],
                );
                self.coins.push(vec![coin]);
            }
        }
        self.coins.clone()
    }

    pub fn create_leadcoin(
        &self,
        sigma1: pallas::Base,
        sigma2: pallas::Base,
        value: u64,
        i: usize,
        c_root_sk: MerkleNode,
        c_path_sk: [MerkleNode; MERKLE_DEPTH_ORCHARD],
        seed: u64,
        sk: SecretKey,
    ) -> LeadCoin {
        // keypair
        let keypair: Keypair = Keypair::new(sk);
        //random commitment blinding values
        let mut rng = thread_rng();
        let c_cm1_blind: DrkValueBlind = pallas::Scalar::random(&mut rng);
        let c_cm2_blind: DrkValueBlind = pallas::Scalar::random(&mut rng);
        let mut tree_cm = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(self.len());
        let c_v = pallas::Base::from(value);
        // coin relative slot index in the epoch
        let c_sl = pallas::Base::from(u64::try_from(i).unwrap());
        //
        //let's assume it's sl for simplicity
        let c_tau = pallas::Base::from(u64::try_from(i).unwrap());
        //

        //let coin_pk_msg = [c_tau, c_root_sk.inner()];
        //let c_pk: pallas::Base = poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init().hash(coin_pk_msg);
        let c_pk: pallas::Point = keypair.public.0;
        let c_pk_coord = c_pk.to_affine().coordinates().unwrap();
        let c_pk_x = c_pk_coord.x();
        let c_pk_y = c_pk_coord.y();

        let c_seed = pallas::Base::from(seed);
        let sn_msg = [c_seed, c_root_sk.inner()];
        let c_sn: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(sn_msg);

        let coin_commit_msg_input =
            [pallas::Base::from(PRF_NULLIFIER_PREFIX), *c_pk_x, *c_pk_y, c_v, c_seed];
        let coin_commit_msg: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<5>, 3, 2>::init()
                .hash(coin_commit_msg_input);
        let c_cm: pallas::Point = pedersen_commitment_base(coin_commit_msg, c_cm1_blind);
        let c_cm_coordinates = c_cm.to_affine().coordinates().unwrap();
        let c_cm_base: pallas::Base = c_cm_coordinates.x() * c_cm_coordinates.y();
        let c_cm_node = MerkleNode::from(c_cm_base);
        tree_cm.append(&c_cm_node.clone());
        let leaf_position = tree_cm.witness();
        let c_root_cm = tree_cm.root(0).unwrap();
        let c_cm_path = tree_cm.authentication_path(leaf_position.unwrap(), &c_root_cm).unwrap();

        let coin_nonce2_msg = [c_seed, c_root_sk.inner()];
        let c_seed2: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(coin_nonce2_msg);

        let coin2_commit_msg_input =
            [pallas::Base::from(PRF_NULLIFIER_PREFIX), *c_pk_x, *c_pk_y, c_v, c_seed2];
        let coin2_commit_msg: pallas::Base =
            poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<5>, 3, 2>::init()
                .hash(coin2_commit_msg_input);
        let c_cm2 = pedersen_commitment_base(coin2_commit_msg, c_cm2_blind);

        // election seeds
        let (y_mu, rho_mu) = self.create_coins_election_seeds(c_sl);
        let coin = LeadCoin {
            value: Some(value),
            cm: Some(c_cm),
            cm2: Some(c_cm2),
            idx: u32::try_from(i).unwrap(), //TODO should be abs slot
            sl: Some(c_sl),
            tau: Some(c_tau),
            nonce: Some(c_seed),
            nonce_cm: Some(c_seed2),
            sn: Some(c_sn),
            keypair: Some(keypair),
            root_cm: Some(mod_r_p(c_root_cm.inner())),
            root_sk: Some(c_root_sk.inner()),
            path: Some(c_cm_path.as_slice().try_into().unwrap()),
            path_sk: Some(c_path_sk),
            c1_blind: Some(c_cm1_blind),
            c2_blind: Some(c_cm2_blind),
            y_mu: Some(y_mu),
            rho_mu: Some(rho_mu),
            sigma1: Some(sigma1),
            sigma2: Some(sigma2),
        };
        coin
    }
    /// see if the participant stakeholder of this epoch is
    /// winning the lottery
    /// if stakeholder with multiple coins have multiple competing winning coins,
    /// only the highest values coin is selected, since the stakeholder can't give more
    /// than a proof per block.
    /// * `sl` - slot relative index
    /// * `idx` - index of the winning coin
    /// returns true if the stakeholder is a leader for the current slot, else otherwise
    pub fn is_leader(&self, sl: u64, idx: &mut usize) -> bool {
        let slusize = sl as usize;
        info!("slot: {}, coin len: {}", sl, self.coins.len());
        assert!(slusize < self.coins.len());
        let competing_coins: &Vec<LeadCoin> = &self.coins.clone()[sl as usize];
        let mut am_leader = vec![];
        let mut highest_stake = 0;
        let mut highest_stake_idx: usize = 0;
        for (winning_idx, coin) in competing_coins.iter().enumerate() {
            let y_exp = [coin.root_sk.unwrap(), coin.nonce.unwrap()];
            let y_exp_hash: pallas::Base =
                poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init(
                )
                .hash(y_exp);
            //TODO (fix) use the hash of y coordinates, using single coordinate is insecure.
            //  pick x coordinate of y for comparison
            let y_coordinates =
                pedersen_commitment_base(coin.y_mu.unwrap(), mod_r_p(y_exp_hash))
                .to_affine()
                .coordinates()
                .unwrap();
            //
            let y_x: pallas::Base = *y_coordinates.x();
            let y_y: pallas::Base = *y_coordinates.y();
            let y_coord_arr = [y_x, y_y];
            let y: pallas::Base =
                poseidon::Hash::<_, poseidon::P128Pow5T3, poseidon::ConstantLength<2>, 3, 2>::init()
                .hash(y_coord_arr);
            //
            let val_2ibig =
                Float10::try_from(coin.value.unwrap()).unwrap().with_precision(RADIX_BITS).value();
            let val_base = pallas::Base::from(coin.value.unwrap());
            let target_base = coin.sigma1.unwrap() * val_base +
                coin.sigma2.unwrap() * val_base * val_base;
            let target_fbig = base2ibig(coin.sigma1.unwrap()) * val_2ibig.clone() + base2ibig(coin.sigma2.unwrap()) * val_2ibig.clone() * val_2ibig;
            let target_ibig = fbig2ibig(target_fbig);
            let y_ibig = base2ibig(y_x);
            info!("y_x: {}, target ibig: {}", y_ibig, target_ibig);
            info!("target base: {:?}", target_base);
            let iam_leader = y < target_base;
            if iam_leader {
                if coin.value.unwrap() > highest_stake {
                    highest_stake = coin.value.unwrap();
                    highest_stake_idx = winning_idx;
                }
                am_leader.push(iam_leader);
            }
        }
        *idx = highest_stake_idx;
        !am_leader.is_empty()
    }

    /// * `sl` - relative slot index (zero based)
    /// * `idx` - idex of the highest winning coin
    /// * `pk` - proving key
    /// returns  the of proof of the winning coin of slot `sl` at index `idx` with
    /// proving key `pk`
    pub fn get_proof(&self, sl: u64, idx: usize, pk: &ProvingKey) -> Proof {
        info!("get_proof");
        let competing_coins: &Vec<LeadCoin> = &self.coins.clone()[sl as usize];
        let coin = competing_coins[idx];
        lead_proof::create_lead_proof(pk, coin).unwrap()
    }
}
