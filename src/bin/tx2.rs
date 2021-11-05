use pasta_curves::{
    arithmetic::{CurveAffine, Field},
    group::{ff::{PrimeField, PrimeFieldBits}, Curve},
    pallas,
};
use halo2_gadgets::{
    ecc::{
        chip::{EccChip, EccConfig},
        FixedPoint, FixedPoints,
    },
};
use rand::rngs::OsRng;

use drk::{
    constants::{
        sinsemilla::{OrchardCommitDomains, OrchardHashDomains, MERKLE_CRH_PERSONALIZATION},
        OrchardFixedBases,
    },
    crypto::pedersen_commitment,
    proof::{Proof, ProvingKey, VerifyingKey},
    spec::i2lebsp,
};

fn mod_r_p(x: pallas::Base) -> pallas::Scalar {
    pallas::Scalar::from_repr(x.to_repr()).unwrap()
}

fn main() {
    let secret = pallas::Base::random(&mut OsRng);
    let public_key = OrchardFixedBases::SpendAuthG.generator() * mod_r_p(secret);
}

