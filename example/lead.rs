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

//use pasta_curves::{arithmetic::CurveAffine, group::Curve};
//use halo2_proofs::arithmetic::CurveAffine;
use pasta_curves::group::{ff::PrimeField, GroupEncoding};

#[derive(Debug, Default, Clone, Copy)]
pub struct Coin {
    value: Option<pallas::Base>, //stake
    cm: Option<pallas::Point>,
    cm2: Option<pallas::Point>,
    cm_blind: Option<pallas::Base>,
    sl: Option<pallas::Base>, //slot id
    tau: Option<pallas::Base>,
    nonce: Option<pallas::Base>,
    nonce_cm: Option<pallas::Point>,
    sn: Option<pallas::Point>, // coin's serial number
    //sk : Option<SecretKey>,
    pk: Option<pallas::Point>,
    root_cm: Option<pallas::Scalar>,
    root_sk: Option<pallas::Scalar>,
    path: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    path_sk: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    opening1: Option<pallas::Base>,
    opening2: Option<pallas::Base>,
}

fn main() {
    let k = 13;
    //
    const LEN: usize = 10;
    let mut rng = thread_rng();
    let mut sks: Vec<u64> = vec![];
    let mut root_sks: Vec<MerkleNode> = vec![];
    let mut path_sks: Vec<[MerkleNode; MERKLE_DEPTH_ORCHARD]> = vec![];
    let mut tree = BridgeTree::<MerkleNode, 32>::new(LEN);
    for i in 0..LEN {
        let tmp: u64 = rng.gen();
        let mut sk: u64 = tmp;
        sks.push(sk.clone());
        let node = MerkleNode(pallas::Base::from(sk));
        tree.append(&node.clone());
        tree.witness();
        let (leaf_pos, path) = tree.authentication_path(&node).unwrap();
        root_sks.push(tree.root().clone());
        path_sks.push(path.as_slice().try_into().unwrap());
    }
    let mut seeds: Vec<u64> = vec![];
    for i in 0..LEN {
        let rho: u64 = rng.gen();
        seeds.push(rho.clone());
    }
    //
    let yu64: u64 = rng.gen();
    let rhou64: u64 = rng.gen();
    let mau_y: pallas::Scalar = pallas::Scalar::from(yu64);
    let mau_rho: pallas::Scalar = pallas::Scalar::from(rhou64);

    //
    let mut coins: Vec<Coin> = vec![];

    //
    let mut tree_cm = BridgeTree::<MerkleNode, 32>::new(LEN);
    let zerou64: u64 = 0;

    for i in 0..LEN {
        let c_v = pallas::Base::from(u64::try_from(i * 2).unwrap());
        //random sampling of the same size of prf,
        //pseudo random sampling that is the size of pederson commitment
        let c_sk: u64 = sks[i];
        let iu64: u64 = u64::try_from(i).unwrap();
        let c_sl = pallas::Base::from(iu64);

        let c_tau = pallas::Base::from(u64::try_from(i).unwrap()); // let's assume it's sl for simplicity
        let c_root_sk: MerkleNode = root_sks[i];

        let c_pk = pedersen_commitment_scalar(mod_r_p(c_tau), mod_r_p(c_root_sk.inner()));

        let c_seed = pallas::Base::from(seeds[i]);
        let c_sn = pedersen_commitment_scalar(mod_r_p(c_seed), mod_r_p(c_root_sk.inner()));
        let c_pk_pt = c_pk.to_affine().coordinates().unwrap();
        let c_cm_message = [*c_pk_pt.x(), *c_pk_pt.y(), c_v.clone(), c_seed.clone()];
        let c_cm_v =
            poseidon::Hash::<_, P128Pow5T3, ConstantLength<4>, 3, 2>::init().hash(c_cm_message);
        let c_cm1_blind = pallas::Base::from(0); //tmp val
        let c_cm2_blind = pallas::Base::from(0); //tmp val
        let c_cm: pallas::Point = pedersen_commitment_scalar(mod_r_p(c_cm_v), mod_r_p(c_cm1_blind));
        //TODO this return run time error! assertion error, it's out of range most likely
        //let c_cm_base_bytes : [u8; 32] = c_cm.to_bytes();
        /*
        let c_cm_base_bytes : [u8; 32] = c_cm.to_affine()
            .coordinates()
            .unwrap()
            .x().to_repr();
        let c_cm_base : pallas::Base = pallas::Base::from_repr(c_cm_base_bytes).unwrap();
        */
        let c_cm_node = MerkleNode(pallas::Base::from(1)); // this is temporary, shouldn't pass of course
        tree_cm.append(&c_cm_node.clone());
        tree_cm.witness();
        let (leaf_pos, c_cm_path) = tree_cm.authentication_path(&c_cm_node).unwrap();
        let c_root_cm = tree_cm.root();
        // lead coin commitment
        let c_seed2 = pedersen_commitment_scalar(mod_r_p(c_seed), mod_r_p(c_root_sk.inner()));
        let c_seed2_pt = c_seed2.to_affine().coordinates().unwrap();
        let lead_coin_msg = [*c_pk_pt.x(), *c_pk_pt.y(), c_v, *c_seed2_pt.x(), *c_seed2_pt.y()];
        let lead_coin_msg_hash =
            poseidon::Hash::<_, P128Pow5T3, ConstantLength<5>, 3, 2>::init().hash(lead_coin_msg);
        let c_cm2 = pedersen_commitment_scalar(mod_r_p(lead_coin_msg_hash), mod_r_p(c_cm2_blind));
        let c_root_sk = root_sks[i];
        let c_path_sk = path_sks[i];
        let coin = Coin {
            value: Some(c_v),
            cm: Some(c_cm),
            cm2: Some(c_cm2),
            cm_blind: Some(c_cm1_blind),
            sl: Some(c_sl),
            tau: Some(c_tau),
            nonce: Some(c_seed),
            nonce_cm: Some(c_seed2),
            sn: Some(c_sn),
            //sk: Some(c_sk),
            pk: Some(c_pk),
            root_cm: Some(mod_r_p(c_root_cm.inner())),
            root_sk: Some(mod_r_p(c_root_sk.inner())),
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

    let po_path = coin.path.unwrap();

    let po_cmp = pallas::Base::from(0);
    // ===============
    let path_sk = path_sks[coin_idx];

    let contract = LeadContract {
        path: coin.path,
        root_sk: coin.root_sk,
        path_sk: Some(path_sk),
        coin_timestamp: coin.tau, //
        coin_nonce: coin.nonce,
        coin_opening_1: Some(mod_r_p(coin.opening1.unwrap())),
        value: coin.value,
        coin_opening_2: Some(mod_r_p(coin.opening2.unwrap())),
        cm_c1_x: Some(*po_cm.x()),
        cm_c1_y: Some(*po_cm.y()),
        cm_c2_x: Some(*po_cm2.x()),
        cm_c2_y: Some(*po_cm2.y()),
        cm_pos: Some(u32::try_from(coin_idx).unwrap()),
        //sn_c1: Some(coin.sn.unwrap()),
        slot: Some(coin.sl.unwrap()),
        mau_rho: Some(mau_rho.clone()),
        mau_y: Some(mau_y.clone()),
        root_cm: Some(coin.root_cm.unwrap()),
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
        po_path[31].inner(), //TODO (res) how the path is structured assumed root is last node in the path.
        po_cmp,
    ];

    let prover = MockProver::run(k, &contract, vec![public_inputs]).unwrap();
    //
    assert_eq!(prover.verify(), Ok(()));
    //
}
