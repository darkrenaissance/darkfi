// https://docs.vocdoni.io/architecture/protocol/anonymous-voting/zk-census-proof.html#protocol-design
use anyhow::Result;
use halo2_gadgets::{
    primitives,
    primitives::poseidon::{ConstantLength, P128Pow5T3},
};
use pasta_curves::{
    arithmetic::{CurveAffine, Field},
    group::{ff::PrimeFieldBits, Curve},
    pallas,
};
use rand::rngs::OsRng;

use drk_halo2::crypto::pedersen_commitment;
use drk_halo2::{constants::sinsemilla::MERKLE_CRH_PERSONALIZATION, spec::i2lebsp};

fn root(path: [pallas::Base; 32], leaf_pos: u32, leaf: pallas::Base) -> pallas::Base {
    let domain = primitives::sinsemilla::HashDomain::new(MERKLE_CRH_PERSONALIZATION);

    let pos_bool = i2lebsp::<32>(leaf_pos as u64);

    let mut node = leaf;
    for (l, (sibling, pos)) in path.iter().zip(pos_bool.iter()).enumerate() {
        let (left, right) = if *pos {
            (*sibling, node)
        } else {
            (node, *sibling)
        };

        let l_star = i2lebsp::<10>(l as u64);
        let left: Vec<_> = left.to_le_bits().iter().by_val().take(255).collect();
        let right: Vec<_> = right.to_le_bits().iter().by_val().take(255).collect();

        let mut message = l_star.to_vec();
        message.extend_from_slice(&left);
        message.extend_from_slice(&right);

        node = domain.hash(message.into_iter()).unwrap();
    }
    node
}

fn main() -> Result<()> {
    // The number of rows in our circuit cannot exceed 2^k
    // let k: u32 = 11;

    // Voter is the owner of the secret key corresponding to a certain zkCensusKey.
    let secret_key = pallas::Base::random(&mut OsRng);
    let zk_census_key =
        primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<1>).hash([secret_key]);

    // Voter's zkCensusKey is included in the census Merkle Tree
    let leaf = zk_census_key.clone();
    let pos = rand::random::<u32>();
    let path: Vec<_> = (0..32).map(|_| pallas::Base::random(&mut OsRng)).collect();
    let merkle_root = root(path.clone().try_into().unwrap(), pos, leaf);

    // The nullifier provided by Voter uniquely corresponds to their secret
    // key and the process ID for a specific voting process.
    let process_id = pallas::Base::from(42);
    let nullifier = primitives::poseidon::Hash::init(P128Pow5T3, ConstantLength::<2>)
        .hash([secret_key, process_id]);

    // The vote itself
    let vote_blind = pallas::Scalar::random(&mut OsRng);
    let vote = pedersen_commitment(1, vote_blind);
    let vote_coords = vote.to_affine().coordinates().unwrap();

    let _public_inputs = [
        merkle_root,
        nullifier,
        process_id,
        *vote_coords.x(),
        *vote_coords.y(),
    ];

    Ok(())
}
