use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};

use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};

use rand::{thread_rng, Rng};

use pasta_curves::{pallas, Fp};

use darkfi::{
    zk:: {
        circuit::lead_contract::{LeadContract},
    },
    crypto::{
        coin::Coin,
        merkle_node::MerkleNode,
        keypair::{Keypair, PublicKey, SecretKey},
        types::*,
        constants::{
            NullifierK, OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV, MERKLE_DEPTH_ORCHARD,
        },
    },
};

use super::{
    nullifier::Nullifier,
    proof::{Proof, ProvingKey, VerifyingKey},
    util::{mod_r_p, pedersen_commitment_scalar, pedersen_commitment_u64},
};

#[derive(Debug,Default,Clone)]
pub struct Coin
{
    let value : Option<u32>, //stake
    let cm : Option<NonIdentityPoint>,
    let cm2 : Option<NonIdentityPoint>,
    let cm_blind : Option<pallas::Base>,
    let sl : Option<u32>, //slot id
    let tau : Option<u32>,
    let nonce : Option<u32>,
    let sn : Option<u32>, // coin's serial number
    let sk : Option<u64>,
    let pk : Option<PublicKey>,
    let root_cm : Option<MerkleNode>,
    let root_sk : Option<MerkleNode>,
    let path: Option<[pallas::Base; MERKLE_DEPTH_ORCHARD]>,
    let path_sk: Option<[pallas::Base; MERKLE_DEPTH_ORCHARD]>,
    let opening1 : Option<pallas::Base>,
    let opening2 : Option<pallas::Base>,
};

fn main()
{
    let k = 13;
    //
    //TODO calculate commitment here
    //this is the commitment of the first coin
    //TODO construct a tree of multiple coins
    const let LEN : u8 = 10;
    let mut rng = thread_rng();
    let sks : Vec<u32> = vec![];
    let root_sks : Vec<MerkleNode> = vec![];
    let path_sks : Option<[MerkleNode;MERKLE_DEPTH_ORCHARD]>;
    let tree  =  BridgeTree::<MerkleNode, 32>::new(LEN);
    for i in LEN {
        let sk : u64 = rng.gen();
        sks.push(sk);
        let node = MerkleNode(sk);
        tree.append(&node);
        let path = tree.authenticate_path(&node);
        root_sks.push(tree.root());
        path_sks.push(path);
    }
    let seeds : Vec<u64> = vec![];
    for i in LEN {
        let rho : u64 = rng.gen();
        seeds.push(rho);
    }
    //
    let mau_y : u64 = rng.gen();
    let mau_rho : u64 = rng.gen();

    //
    let coins : Vec<Coin> = vec![Coin];

    //
    let tree_cm = BridgeTree<MerkleNode, 32>::new(LEN);
    for i in LEN {
        let c_v = i*2;
        //random sampling of the same size of prf,
        //pseudo random sampling that is the size of pederson commitment
        let c_sk : u64 = sks[i];
        let c_sl : u32 = i;
        let c_tau : u32 = i; // let's assume it's sl for simplicity
        let c_root_sk : MerkleNode  = root_sks[i];
        let c_seed : u64 = seeds[i];
        let c_sn : u32 = pedersen_commitment_u64(c_seed, c_root_sk);
        let c_cm_message = [c_pk.clone(), c_v.clone(), c_seed.clone()];
        let c_cm_v = poseidon::Hash::<_,P128Pow5T3, ConstantLength<6>, 3, 2>::init().hash(c_cm_message);
        let c_cm1_blind = pallas::Base::from(0); //tmp val
        let c_cm2_blind = pallas::Base::from(0); //tmp val
        let c_cm : NonIdentityPoint  = pedersen_commitment_base(c_cm_v, c_cm1_blind);
        let c_pk = PublicKey::from_secret(c_sk);
        let c_cm_node = MerkleNode(c_cm);
        tree_cm.append(&c_cm_node);
        let c_cm_path = tree_cm.authenticate_path(&c_cm_node).unwrap();
        let c_root_cm = tree_cm.root();
        // lead coin commitment
        //TODO this c_v can be
        let c_seed2 = pedersen_commitment_u64(c_seed, c_root_sk);
        let lead_coin_msg = [c_pk, c_v, c_seed2];
        poseidon::Hash::<_,P128Pow5T3, ConstantLength<6>, 3, 2>::init().hash(lead_coin_msg);
        let c_cm2 = pedersen_commitment_u64(lead_coin_msg, c_seeed2);
        let coin  = Coin {
            c_v,
            c_cm,
            c_cm2,
            c_cm_blind,
            c_sl,
            c_tau,
            c_seed,
            c_tau,
            c_sn,
            c_sk,
            c_pk,
            c_root_cm,
            root_sks[i],
            c_cm_path,
            c_path_sk,
            c_cm1_blind,
            c_cm2_blind,
        };
        coins.push(coin);
    }

    let coin_idx = 0;
    let coin = coins[coin_idx];
    let path_sk = path_sks[coin_idx];
    let contract = LeadContract {
        coin.path,
        coin.root_sk,
        path_sk,
        coin.tau, //
        coin.nonce,
        coin.opening1,
        coin.value,
        coin.opening2,
        coin.cm,
        coin.cm2,
        coin.sn,
        coin.sl,
        mau_rho.clone(),
        mau_y.clone(),
        coin.c_root_cm,
    };
    //public inputs
    let mut public_inputs = vec![];
    //TODO
    let prover = MockProver::run(k, &contract, vec![public_inputs]).unwrap();
    assert_eq!(prover.verify(), Ok(()));
}
