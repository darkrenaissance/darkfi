use halo2_proofs::{arithmetic::Field, dev::MockProver, circuit::Value};
use incrementalmerkletree::{bridgetree::BridgeTree, Tree};
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::PrimeField, Curve},
    pallas,
};

use rand::{thread_rng, Rng};

use darkfi::{
    crypto::{
        constants::MERKLE_DEPTH_ORCHARD,
        leadcoin::LeadCoin,
        merkle_node::MerkleNode,
        util::{mod_r_p, pedersen_commitment_scalar},
    },
    zk::circuit::lead_contract::LeadContract,
};


const MERKLE_DEPTH: u8 = MERKLE_DEPTH_ORCHARD as u8;

fn create_coins_sks(len: usize) -> (Vec<MerkleNode>, Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]>) {
    /*
    at the onset of an epoch, the first slot's coin's secret key
    is sampled at random, and the reset of the secret keys are derived,
    for sk (secret key) at time i+1 is derived from secret key at time i.
     */
    let mut rng = thread_rng();
    let sk: u64 = rng.gen();
    let mut tree = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(len);
    let mut root_sks: Vec<MerkleNode> = vec![];
    let mut path_sks: Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]> = vec![];
    for _i in 0..len {
        //TODO (research) why the conversion between point and base is panicing?
        // is the endianess different?
        let base = pedersen_commitment_scalar(pallas::Scalar::one(), pallas::Scalar::from(sk));
        let _coord = base.to_affine().coordinates().unwrap();
        //let sk =  coord.x() * coord.y();
        //let sk =  *coord.y();
        let sk: [u8; 32] = pallas::Base::random(rng.clone()).to_repr();
        let node = MerkleNode::from_bytes(&sk).unwrap();
        //let serialized = serde_json::to_string(&node).unwrap();
        //println!("serialized: {}", serialized);
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

fn create_coins(
    root_sks: Vec<MerkleNode>,
    path_sks: Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    values: Vec<u64>,
    cm1_blind: pallas::Base,
    cm2_blind: pallas::Base,
    len: usize,
) -> Vec<LeadCoin> {
    let mut rng = thread_rng();
    let mut seeds: Vec<u64> = vec![];
    for _i in 0..len {
        let rho: u64 = rng.gen();
        seeds.push(rho);
    }

    let mut tree_cm = BridgeTree::<MerkleNode, MERKLE_DEPTH>::new(len);
    let mut coins: Vec<LeadCoin> = vec![];
    for i in 0..len {
        let c_v = pallas::Base::from(values[i]);
        //random sampling of the same size of prf,
        //pseudo random sampling that is the size of pederson commitment

        // coin slot number
        let c_sl = pallas::Base::from(u64::try_from(i).unwrap());

        //
        let c_tau = pallas::Base::from(u64::try_from(i).unwrap()); // let's assume it's sl for simplicity
                                                                   //
        let c_root_sk: MerkleNode = root_sks[i];

        let c_pk = pedersen_commitment_scalar(mod_r_p(c_tau), mod_r_p(c_root_sk.inner()));

        let c_seed = pallas::Base::from(seeds[i]);
        let c_sn = pedersen_commitment_scalar(mod_r_p(c_seed), mod_r_p(c_root_sk.inner()));
        let c_pk_pt = c_pk.to_affine().coordinates().unwrap();
        let c_pk_pt_x: pallas::Base = *c_pk_pt.x();
        let c_pk_pt_y: pallas::Base = *c_pk_pt.y();

        let c_cm_v = c_v * c_seed * c_pk_pt_x * c_pk_pt_y;
        let c_cm1_blind = cm1_blind; //TODO (fix) should be read from DrkValueBlind
        let c_cm2_blind = cm2_blind; //TODO (fix) should be read from DrkValueBlind
        let c_cm: pallas::Point = pedersen_commitment_scalar(mod_r_p(c_cm_v), mod_r_p(c_cm1_blind));

        let c_cm_coordinates = c_cm.to_affine().coordinates().unwrap();
        let c_cm_base: pallas::Base = c_cm_coordinates.x() * c_cm_coordinates.y();
        let c_cm_node = MerkleNode(c_cm_base);
        tree_cm.append(&c_cm_node.clone());
        let leaf_position = tree_cm.witness();
        let c_root_cm = tree_cm.root(0).unwrap();
        let c_cm_path = tree_cm.authentication_path(leaf_position.unwrap(), &c_root_cm).unwrap();
        // lead coin commitment
        let c_seed2 = pedersen_commitment_scalar(mod_r_p(c_seed), mod_r_p(c_root_sk.inner()));
        let c_seed2_pt = c_seed2.to_affine().coordinates().unwrap();
        /*
            let lead_coin_msg = [c_pk_pt_y.clone(),
            c_pk_pt_x.clone(),
            c_v,
             *c_seed2_pt.x(),
             *c_seed2_pt.y()
        ];
            let lead_coin_msg_hash =
            poseidon::Hash::<_, P128Pow5T3, ConstantLength<5>, 3, 2>::init().hash(lead_coin_msg);
             */
        let lead_coin_msg = c_pk_pt_y * c_pk_pt_x * c_v * *c_seed2_pt.x() * *c_seed2_pt.y();
        let c_cm2 = pedersen_commitment_scalar(mod_r_p(lead_coin_msg), mod_r_p(c_cm2_blind));
        let c_root_sk = root_sks[i];

        let c_root_sk_bytes: [u8; 32] = c_root_sk.inner().to_repr();
        let mut c_root_sk_base_bytes: [u8; 32] = [0; 32];
        c_root_sk_base_bytes[..23].copy_from_slice(&c_root_sk_bytes[..23]);
        let _c_root_sk_base = pallas::Base::from_repr(c_root_sk_base_bytes);

        let c_path_sk = path_sks[i];

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
            pk_x: Some(c_pk_pt_x),
            pk_y: Some(c_pk_pt_y),
            root_cm: Some(mod_r_p(c_root_cm.inner())),
            root_sk: Some(c_root_sk.inner()),
            path: Some(c_cm_path.as_slice().try_into().unwrap()),
            path_sk: Some(c_path_sk),
            opening1: Some(c_cm1_blind),
            opening2: Some(c_cm2_blind),
        };
        coins.push(coin);
    }
    coins
}

fn main() {
    let k: u32 = 13;
    //let lead_pk = ProvingKey::build(k, &LeadContract::default());
    //let lead_vk = VerifyingKey::build(k, &LeadContract::default());
    //
    const LEN: usize = 10;
    let mut rng = thread_rng();
    let mut _root_sks: Vec<MerkleNode> = vec![];
    let mut _path_sks: Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]> = vec![];
    let mut values: Vec<u64> = vec![];
    for i in 0..LEN {
        values.push(u64::try_from(i * 2).unwrap());
    }
    let cm1_val: u64 = rng.gen();
    let cm1_blind: pallas::Base = pallas::Base::from(cm1_val);
    let cm2_val: u64 = rng.gen();
    let cm2_blind: pallas::Base = pallas::Base::from(cm2_val);
    (_root_sks, _path_sks) = create_coins_sks(LEN);
    let coins: Vec<LeadCoin> =
        create_coins(_root_sks, _path_sks, values, cm1_blind, cm2_blind, LEN);
    let coin_idx = 0;
    let coin = coins[coin_idx];

    let yu64: u64 = rng.gen();
    let rhou64: u64 = rng.gen();
    let mau_y: pallas::Base = pallas::Base::from(yu64);
    let mau_rho: pallas::Base = pallas::Base::from(rhou64);

    let contract = LeadContract {
        path: Value::known(coin.path.unwrap()),
        coin_pk_x: Value::known(coin.pk_x.unwrap()),
        coin_pk_y: Value::known(coin.pk_y.unwrap()),
        root_sk: Value::known(coin.root_sk.unwrap()),
        sf_root_sk: Value::known(mod_r_p(coin.root_sk.unwrap())),
        path_sk: Value::known(coin.path_sk.unwrap()),
        coin_timestamp: Value::known(coin.tau.unwrap()), //
        coin_nonce: Value::known(coin.nonce.unwrap()),
        coin_opening_1: Value::known(mod_r_p(coin.opening1.unwrap())),
        value: Value::known(coin.value.unwrap()),
        coin_opening_2: Value::known(mod_r_p(coin.opening2.unwrap())),
        cm_pos: Value::known(coin.idx),
        //sn_c1: Value::known(coin.sn.unwrap()),
        slot: Value::known(coin.sl.unwrap()),
        mau_rho: Value::known(mod_r_p(mau_rho)),
        mau_y: Value::known(mod_r_p(mau_y)),
        root_cm: Value::known(coin.root_cm.unwrap()),
    };

    //let proof = create_lead_proof(lead_pk.clone(), coin.clone()).unwrap();
    //verify_lead_proof(&lead_vk, &proof, coin);

    // calculate public inputs
    let public_inputs = coin.public_inputs();

    let prover = MockProver::run(k, &contract, vec![public_inputs]).unwrap();
    //
    assert_eq!(prover.verify(), Ok(()));
    //
}
