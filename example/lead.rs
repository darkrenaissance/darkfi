use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};

use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};

use halo2_proofs::dev::MockProver;

use rand::{thread_rng, Rng};

use pasta_curves::{pallas, Fp};

use darkfi::{
    crypto::{
        constants::{
            NullifierK, OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV,
            MERKLE_DEPTH_ORCHARD,
        },
        keypair::{Keypair, PublicKey, SecretKey},
        merkle_node::MerkleNode,
        nullifier::Nullifier,
        proof::{Proof, ProvingKey, VerifyingKey},
        types::*,
        util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
    },
    zk::circuit::lead_contract::LeadContract,
};

use incrementalmerkletree::Hashable;

use pasta_curves::{
    arithmetic::CurveAffine,
    group::{ff::PrimeField, Curve, GroupEncoding},
};
//use halo2_proofs::arithmetic::CurveAffine;

#[derive(Debug, Default, Clone, Copy)]
pub struct Coin {
    value: Option<pallas::Base>, //stake
    cm: Option<pallas::Point>,
    cm2: Option<pallas::Point>,
    sl: Option<pallas::Base>, //slot id
    tau: Option<pallas::Base>,
    nonce: Option<pallas::Base>,
    nonce_cm: Option<pallas::Point>,
    sn: Option<pallas::Point>, // coin's serial number
    //sk : Option<SecretKey>,
    pk: Option<pallas::Point>,
    pk_x: Option<pallas::Base>,
    pk_y: Option<pallas::Base>,
    root_cm: Option<pallas::Scalar>,
    root_sk: Option<pallas::Base>,
    path: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    path_sk: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    opening1: Option<pallas::Base>,
    opening2: Option<pallas::Base>,
}

fn create_coins_sks(len : usize) ->
    (Vec<MerkleNode>, Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]>)
{
    /*
    at the onset of an epoch, the first slot's coin's secret key
    is sampled at random, and the reset of the secret keys are derived,
    for sk (secret key) at time i+1 is derived from secret key at time i.
     */
    let mut rng = thread_rng();
    let sk: u64 = rng.gen();
    let mut tree = BridgeTree::<MerkleNode, 32>::new(len);
    let mut root_sks: Vec<MerkleNode> = vec![];
    let mut path_sks: Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]> = vec![];
    for i in 0..len {
        //TODO (research) why the conversion between point and base is panicing?
        // is the endianess different?
        let base = pedersen_commitment_scalar(pallas::Scalar::one(), pallas::Scalar::from(sk));
        let coord = base.to_affine().coordinates().unwrap();
        let coord_prod =  coord.x() * coord.y();
        let node = MerkleNode(coord_prod);
        tree.append(&node.clone());
        let leaf_position = tree.witness();
        //let (leaf_pos, path) = tree.authentication_path(leaf_position.unwrap()).unwrap();
        let path = tree.authentication_path(leaf_position.unwrap()).unwrap();
        root_sks.push(tree.root().clone());
        path_sks.push(path.as_slice().try_into().unwrap());
    }
    (root_sks, path_sks)
}

/*
fn create_coins(...)
{

}

fn build_commit_tree(cms : Vec<Coin>)
{
    //
}

*/
fn main() {
    let k = 13;
    //
    const LEN: usize = 10;
    let mut rng = thread_rng();
    let mut root_sks: Vec<MerkleNode> = vec![];
    let mut path_sks: Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]> = vec![];
    (root_sks, path_sks) = create_coins_sks(LEN);
    /*

    for i in 0..LEN {
        let sk: u64 = rng.gen();
        let node = MerkleNode(pallas::Base::from(sk));
        tree.append(&node.clone());
        let leaf_position = tree.witness();
        //let (leaf_pos, path) = tree.authentication_path(leaf_position.unwrap()).unwrap();
        let path = tree.authentication_path(leaf_position.unwrap()).unwrap();
        root_sks.push(tree.root().clone());
        path_sks.push(path.as_slice().try_into().unwrap());
    }
    */
    let mut seeds: Vec<u64> = vec![];
    for i in 0..LEN {
        let rho: u64 = rng.gen();
        seeds.push(rho.clone());
    }
    //
    let yu64: u64 = rng.gen();
    let rhou64: u64 = rng.gen();
    let mau_y: pallas::Base = pallas::Base::from(yu64);
    let mau_rho: pallas::Base = pallas::Base::from(rhou64);

    //
    let mut coins: Vec<Coin> = vec![];

    //
    let mut tree_cm = BridgeTree::<MerkleNode, 32>::new(LEN);
    let zerou64: u64 = 0;

    for i in 0..LEN {
        let c_v = pallas::Base::from(u64::try_from(i * 2).unwrap());
        //random sampling of the same size of prf,
        //pseudo random sampling that is the size of pederson commitment

        let iu64: u64 = u64::try_from(i).unwrap();
        let c_sl = pallas::Base::from(iu64);

        let c_tau = pallas::Base::from(u64::try_from(i).unwrap()); // let's assume it's sl for simplicity
        let c_root_sk: MerkleNode = root_sks[i];

        let c_pk = pedersen_commitment_scalar(mod_r_p(c_tau), mod_r_p(c_root_sk.inner()));

        let c_seed = pallas::Base::from(seeds[i]);
        let c_sn = pedersen_commitment_scalar(mod_r_p(c_seed), mod_r_p(c_root_sk.inner()));
        let c_pk_pt = c_pk.to_affine().coordinates().unwrap();
        let c_pk_pt_x: pallas::Base = *c_pk_pt.x();
        let c_pk_pt_y: pallas::Base = *c_pk_pt.y();

        let c_cm_v = c_v.clone() * c_seed.clone() * c_pk_pt_x * c_pk_pt_y;
        let c_cm1_blind = pallas::Base::from(1); //tmp val
        let c_cm2_blind = pallas::Base::from(1); //tmp val
        let c_cm: pallas::Point = pedersen_commitment_scalar(mod_r_p(c_cm_v), mod_r_p(c_cm1_blind));

        let c_cm_coordinates = c_cm.to_affine().coordinates().unwrap();
        let c_cm_base: pallas::Base = c_cm_coordinates.x() * c_cm_coordinates.y();
        let c_cm_node = MerkleNode(c_cm_base);
        tree_cm.append(&c_cm_node.clone());
        let leaf_position = tree_cm.witness();
        let c_cm_path = tree_cm.authentication_path(leaf_position.unwrap()).unwrap();
        let c_root_cm = tree_cm.root();
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
        let lead_coin_msg =
            c_pk_pt_y.clone() * c_pk_pt_x.clone() * c_v * *c_seed2_pt.x() * *c_seed2_pt.y();
        let c_cm2 = pedersen_commitment_scalar(mod_r_p(lead_coin_msg), mod_r_p(c_cm2_blind));
        let c_root_sk = root_sks[i];

        let c_root_sk_bytes: [u8; 32] = c_root_sk.inner().to_repr();
        let mut c_root_sk_base_bytes: [u8; 32] = [0; 32];
        for i in 0..23 {
            c_root_sk_base_bytes[i] = c_root_sk_bytes[i];
        }
        let c_root_sk_base = pallas::Base::from_repr(c_root_sk_base_bytes);

        let c_path_sk = path_sks[i];

        let coin = Coin {
            value: Some(c_v),
            cm: Some(c_cm),
            cm2: Some(c_cm2),
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

    // ================
    // public inputs
    // ================
    let coin_idx = 0;
    let coin = coins[coin_idx];

    let po_nonce = coin.nonce_cm.unwrap().to_affine().coordinates().unwrap();

    let po_nonce = coin.nonce_cm.unwrap().to_affine().coordinates().unwrap();

    let po_tau = pedersen_commitment_scalar(mod_r_p(coin.tau.unwrap()), coin.root_cm.unwrap())
        .to_affine()
        .coordinates()
        .unwrap();

    let po_cm = coin.cm.unwrap().to_affine().coordinates().unwrap();
    let po_cm2 = coin.cm2.unwrap().to_affine().coordinates().unwrap();

    let po_pk = coin.pk.unwrap().to_affine().coordinates().unwrap();
    let po_sn = coin.sn.unwrap().to_affine().coordinates().unwrap();

    let po_cmp = pallas::Base::from(0);
    let zero = pallas::Base::from(0);
    // ===============
    let path_sk = path_sks[coin_idx];
    let cm_pos = u32::try_from(coin_idx).unwrap();
    let contract = LeadContract {
        path: coin.path,
        coin_pk_x: coin.pk_x,
        coin_pk_y: coin.pk_y,
        root_sk: coin.root_sk,
        path_sk: Some(path_sk),
        coin_timestamp: coin.tau, //
        coin_nonce: coin.nonce,
        coin_opening_1: Some(mod_r_p(coin.opening1.unwrap())),
        value: coin.value,
        coin_opening_2: Some(mod_r_p(coin.opening2.unwrap())),
        cm_pos: Some(cm_pos),
        //sn_c1: Some(coin.sn.unwrap()),
        slot: Some(coin.sl.unwrap()),
        mau_rho: Some(mau_rho.clone()),
        mau_y: Some(mau_y.clone()),
        root_cm: Some(coin.root_cm.unwrap()),
    };

    let cm_root = {
        let pos: u32 = cm_pos;
        let c_cm_coordinates = coin.cm.unwrap().to_affine().coordinates().unwrap();
        let c_cm_base: pallas::Base = c_cm_coordinates.x() * c_cm_coordinates.y();
        let mut current = MerkleNode(c_cm_base);
        for (level, sibling) in coin.path.unwrap().iter().enumerate() {
            let level = level as u8;
            current = if pos & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current
    };

    let mut public_inputs: Vec<pallas::Base> = vec![
        *po_nonce.x(),
        *po_nonce.y(),
        *po_pk.x(),
        *po_pk.y(),
        *po_sn.x(),
        *po_sn.y(),
        *po_cm.x(),
        *po_cm.y(),
        *po_cm2.x(),
        *po_cm2.y(),
        cm_root.0,
        po_cmp,
    ];

    let prover = MockProver::run(k, &contract, vec![public_inputs]).unwrap();
    //
    assert_eq!(prover.verify(), Ok(()));
    //
}
