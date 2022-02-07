use bitvec::prelude::*;
use halo2_gadgets::primitives::{
    poseidon,
    poseidon::{ConstantLength, P128Pow5T3},
};
use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};
use pasta_curves::{
    arithmetic::{CurveAffine, Field, FieldExt},
    group::{Curve, Group},
    pallas,
};
use rand::rngs::OsRng;
use simplelog::{ColorChoice::Auto, Config, LevelFilter, TermLogger, TerminalMode::Mixed};

use darkfi::{
    crypto::{
        keypair::Keypair,
        merkle_node::MerkleNode,
        schnorr::SchnorrSecret,
        util::{mod_r_p, pedersen_commitment_scalar},
    },
    Result,
};

fn main() -> Result<()> {
    let loglevel = match option_env!("RUST_LOG") {
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        Some(_) | None => LevelFilter::Info,
    };
    TermLogger::init(loglevel, Config::default(), Mixed, Auto)?;

    /*
    let bincode = include_bytes!("../proof/dao.zk.bin");
    let zkbin = ZkBinary::decode(bincode)?;
    */

    // Contract address
    let a = pallas::Base::random(&mut OsRng);
    // Money in treasury
    let t = pallas::Base::from(666);
    // Serial number
    let s = pallas::Base::random(&mut OsRng);
    // Bulla blind
    let b_b = pallas::Base::random(&mut OsRng);

    let message = [a, t, s, b_b];
    let hasher = poseidon::Hash::init(P128Pow5T3, ConstantLength::<4>);
    let bulla = hasher.hash(message);

    // Merkle tree of DAOs
    let mut tree = BridgeTree::<MerkleNode, 32>::new(100);
    let dao0 = pallas::Base::random(&mut OsRng);
    let dao2 = pallas::Base::random(&mut OsRng);
    tree.append(&MerkleNode(dao0));
    tree.witness();
    tree.append(&MerkleNode(bulla));
    tree.witness();
    tree.append(&MerkleNode(dao2));
    tree.witness();

    let (leaf_pos, merkle_path) = tree.authentication_path(&MerkleNode(bulla)).unwrap();
    let leaf_pos: u64 = leaf_pos.into();
    let leaf_pos = leaf_pos as u32;

    // Output 0:
    let output0_val = 42_u64;
    let output0_dest = pallas::Point::random(&mut OsRng);
    let output0_coords = output0_dest.to_affine().coordinates().unwrap();
    let output0_blind = pallas::Base::random(&mut OsRng);

    let message =
        [pallas::Base::from(output0_val), *output0_coords.x(), *output0_coords.y(), output0_blind];
    let hasher = poseidon::Hash::init(P128Pow5T3, ConstantLength::<4>);
    let output0 = hasher.hash(message);

    let authority = Keypair::random(&mut OsRng);
    let signature = authority.secret.sign(&output0.to_bytes());

    let vote_1 = pallas::Base::from(44);
    let vote_2 = pallas::Base::from(13);
    // This is a NO vote
    let vote_3 = -pallas::Base::from(49);

    let vote_1_blind = pallas::Scalar::random(&mut OsRng);
    let vote_1_commit = pedersen_commitment_scalar(mod_r_p(vote_1), vote_1_blind);

    let vote_2_blind = pallas::Scalar::random(&mut OsRng);
    let vote_2_commit = pedersen_commitment_scalar(mod_r_p(vote_2), vote_2_blind);

    let vote_3_blind = pallas::Scalar::random(&mut OsRng);
    let vote_3_commit = pedersen_commitment_scalar(mod_r_p(vote_3), vote_3_blind);

    let vote_commit = vote_1_commit + vote_2_commit + vote_3_commit;
    let vote_blinds = vote_1_blind + vote_2_blind + vote_3_blind;
    let vote_commit_coords = vote_commit.to_affine().coordinates().unwrap();

    /*
    let number = pallas::Base::from(u64::MAX).to_bytes();
    let bits = number.view_bits::<Lsb0>();
    println!("Positive: {:?}", bits);

    //let number = (-pallas::Base::from(u64::MAX)).to_bytes();
    let number = pallas::Base::from(0).to_bytes();
    let bits = number.view_bits::<Lsb0>();
    println!("Negative: {:?}", bits);
    */

    Ok(())
}
