use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};

use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};

use halo2_proofs::{
    dev::MockProver,
};

use rand::{thread_rng, Rng};

use pasta_curves::{pallas, Fp};

use darkfi::{
    zk:: {
        circuit::lead_contract::{LeadContract},
    },
    crypto::{
        merkle_node::MerkleNode,
        keypair::{Keypair, PublicKey, SecretKey},
        types::*,
        constants::{
            NullifierK, OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV, MERKLE_DEPTH_ORCHARD,
        },
        nullifier::Nullifier,
        proof::{Proof, ProvingKey, VerifyingKey},
        util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
    },
};



#[derive(Debug,Default,Clone)]
pub struct Coin
{
    value : Option<pallas::Base>, //stake
    cm : Option<pallas::Point>,
    cm2 : Option<pallas::Point>,
    cm_blind : Option<pallas::Base>,
    sl : Option<pallas::Base>, //slot id
    tau : Option<pallas::Base>,
    nonce : Option<pallas::Base>,
    sn : Option<pallas::Base>, // coin's serial number
    sk : Option<SecretKey>,
    pk : Option<PublicKey>,
    root_cm : Option<pallas::Scalar>,
    root_sk : Option<pallas::Scalar>,
    path: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    path_sk: Option<[MerkleNode; MERKLE_DEPTH_ORCHARD]>,
    opening1 : Option<pallas::Base>,
    opening2 : Option<pallas::Base>,
}

fn main()
{
    let k = 13;
    //
    //TODO calculate commitment here
    //this is the commitment of the first coin
    //TODO construct a tree of multiple coins
    const LEN : usize = 10;
    let mut rng = thread_rng();
    let sks : Vec<u64> = vec![];
    let root_sks : Vec<MerkleNode> = vec![];
    let path_sks : Vec<[MerkleNode;MERKLE_DEPTH_ORCHARD]> = vec![];
    let tree  =  BridgeTree::<MerkleNode, 32>::new(LEN);
    for i in 0..LEN {
        let tmp : u64 = rng.gen();
        let sk : u64 = tmp;
        sks.push(sk);
        let node = MerkleNode(pallas::Base::from(sk));
        tree.append(&node);
        let (leaf_pos, path) = tree.authentication_path(&node).unwrap();
        root_sks.push(tree.root());
        path_sks.push(path);
    }
    let seeds : Vec<u64> = vec![];
    for i in 0..LEN {
        let rho : u64 = rng.gen();
        seeds.push(rho);
    }
    //
    let mau_y : pallas::Scalar = pallas::Scalar::from(rng.gen());
    let mau_rho : pallas::Scalar = pallas::Scalar::from(rng.gen());

    //
    let coins : Vec<Coin> = vec![];

    //
    let tree_cm = BridgeTree::<MerkleNode, 32>::new(LEN);
    let zerou64 : u64 = 0;
    for i in 0..LEN {
        let c_v = pallas::Base::from(u64::try_from(i*2).unwrap());
        //random sampling of the same size of prf,
        //pseudo random sampling that is the size of pederson commitment
        let c_sk : u64 = sks[i];
        let iu64 : u64 = u64::try_from(i).unwrap();
        let c_sl  = pallas::Base::from(iu64);

        //TODO 512 secret-key/public-key to cop with pallas curves
        //note! sk is used in MerkleNode takes pallas::Base as input
        //while the pallas::base is 512, the SecretKey is  of size 256, a larger keyring is needed
        //TODO what is the endianess of this keyring
        let sk_bits = vec![];
        sk_bits.append(&mut c_sk.to_le_bytes().to_vec());
        sk_bits.append(&mut zerou64.to_le_bytes().to_vec());
        sk_bits.append(&mut zerou64.to_le_bytes().to_vec());
        sk_bits.append(&mut zerou64.to_le_bytes().to_vec());
        let c_pk = PublicKey::from_secret(SecretKey::from_bytes(sk_bits.as_slice().try_into().unwrap()).unwrap());
        let c_tau  = pallas::Base::from(u64::try_from(i).unwrap()); // let's assume it's sl for simplicity
        let c_root_sk : MerkleNode  = root_sks[i];
        let c_seed  = pallas::Base::from(seeds[i]);
        let c_sn  = pedersen_commitment_base(c_seed, c_root_sk);
        let c_cm_message = [c_pk.clone(), c_v.clone(), c_seed.clone()];
        let c_cm_v = poseidon::Hash::<_,P128Pow5T3, ConstantLength<6>, 3, 2>::init().hash(c_cm_message);
        let c_cm1_blind = pallas::Base::from(0); //tmp val
        let c_cm2_blind = pallas::Base::from(0); //tmp val
        let c_cm : pallas::Point  = pedersen_commitment_scalar(c_cm_v, c_cm1_blind);
        let c_cm_node = MerkleNode(c_cm);
        tree_cm.append(&c_cm_node);
        let (leaf_pos, c_cm_path) = tree_cm.authentication_path(&c_cm_node).unwrap();
        let c_root_cm = tree_cm.root();
        // lead coin commitment
        //TODO this c_v can be
        let c_seed2 = pedersen_commitment_u64(c_seed, c_root_sk);
        let lead_coin_msg = [c_pk, c_v, c_seed2];
        poseidon::Hash::<_,P128Pow5T3, ConstantLength<6>, 3, 2>::init().hash(lead_coin_msg);
        let c_cm2 = pedersen_commitment_u64(lead_coin_msg, c_seed2);
        let c_root_sk = root_sks[i];
        let c_path_sk = path_sks[i];
        let coin  = Coin {
            value: Some(c_v),
            cm: Some(c_cm),
            cm2: Some(c_cm2),
            cm_blind: Some(c_cm1_blind),
            sl: Some(c_sl),
            tau: Some(c_tau),
            nonce: Some(c_seed),
            sn:  Some(c_sn),
            sk: Some(c_sk),
            pk: Some(c_pk),
            root_cm: Some(c_root_cm),
            root_sk: Some(c_root_sk),
            path: Some(c_cm_path),
            path_sk: Some(c_path_sk),
            opening1: Some(c_cm1_blind),
            opening2: Some(c_cm2_blind),
        };
        coins.push(coin);
    }

    let coin_idx = 0;
    let coin = coins[coin_idx];
    let path_sk = path_sks[coin_idx];
    let contract = LeadContract {
        path: Some(coin.path),
        root_sk: Some(coin.root_sk), //TODO where doesn' this come from?
        path_sk: Some(path_sk),
        coin_timestamp: Some(pallas::Base::from(coin.tau)), //
        coin_nonce: Some(pallas::Base::from(coin.nonce)),
        coin_opening_1: Some(coin.opening1),
        value: Some(coin.value),
        coin_opening_2: Some(coin.opening2),
        cm_c1_x: Some(coin.cm.0.x),
        cm_c1_y: Some(coin.cm.0.y),
        cm_c2_x: Some(coin.cm2.0.x),
        cm_c2_y: Some(coin.cm2.0.y),
        cm_pos : Some(coin_idx.unwrap()),
        sn_c1: Some(coin.sn.unwrap()),
        slot: Some(coin.sl.unwrap()),
        mau_rho: Some(mau_rho.clone()),
        mau_y: Some(mau_y.clone()),
        root_cm: Some(coin.root_cm.unwrap()),
    };
    //public inputs
    let c0 = pedersen_commitment_scalar(mod_r_p(coin.nonce.unwrap()), coin.root_cm.unwrap());
    let c1 = pedersen_commitment_scalar(mod_r_p(coin.tau.unwrap()), coin.root_cm.unwrap());
    //TODO root_cm need to be converted to Fp
    let c2 = pedersen_commitment_scalar(mod_r_p(coin.nonce.unwrap()), coin.root_cm.unwrap());
    let c3 = coin.cm.unwrap();
    let c4 = coin.cm2.unwrap();
    let c5 = coin.path.unwrap();
    let c6 = pallas::Base::from(0);
    let mut public_inputs = vec![c0.x, c0.y,
                                 c1.x, c1.y,
                                 c2.x, c2.y,
                                 c3.x, c3.y,
                                 c4.x(), c4.y(),
                                 c5,
                                 c6,
    ];
    //TODO
    let prover = MockProver::run(k, &contract, vec![public_inputs]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}
