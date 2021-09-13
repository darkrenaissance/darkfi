use std::iter;

use group::{ff::PrimeFieldBits, Curve};
use halo2::{
    arithmetic::{CurveAffine, Field, FieldExt},
    pasta::{Fp, Fq},
};
use halo2_ecc::gadget::FixedPoints;
use halo2_poseidon::primitive::{ConstantLength, Hash, P128Pow5T3 as OrchardNullifier};
use orchard::constants::{fixed_bases::OrchardFixedBases, sinsemilla::MERKLE_CRH_PERSONALIZATION};
use rand::rngs::OsRng;
use sinsemilla::primitive::{CommitDomain, HashDomain};

use halo2_examples::pedersen_commitment;

fn main() {
    let secret_key = Fq::random(&mut OsRng);
    let serial = Fp::random(&mut OsRng);

    // Sinsemilla hash
    let domain = HashDomain::new(MERKLE_CRH_PERSONALIZATION);
    let nullifier = domain
        .hash(
            iter::empty()
                .chain(secret_key.to_le_bits().iter().by_val())
                .chain(serial.to_le_bits().iter().by_val()),
        )
        .unwrap();

    let public_key = OrchardFixedBases::SpendAuthG.generator() * secret_key;
    let coords = public_key.to_affine().coordinates().unwrap();

    let value = 110;
    let asset = 1;

    let value_blind = Fq::random(&mut OsRng);
    let asset_blind = Fq::random(&mut OsRng);

    let coin_blind = Fp::random(&mut OsRng);

    // FIXME:
    let messages = [
        [*coords.x(), *coords.y()],
        [Fp::from(value), Fp::from(asset)],
        [serial, coin_blind],
    ];
    let mut coin = Fp::zero();
    for msg in messages.iter() {
        coin += Hash::init(OrchardNullifier, ConstantLength::<2>).hash(*msg);
    }

    // TODO: Merkle

    let value_commit = pedersen_commitment(value, value_blind);
    let asset_commit = pedersen_commitment(asset, asset_blind);
    let value_coords = value_commit.to_affine().coordinates().unwrap();
    let asset_coords = value_commit.to_affine().coordinates().unwrap();

    let sig_secret = Fq::random(&mut OsRng);
    let sig_pubkey = OrchardFixedBases::SpendAuthG.generator() * sig_secret;
    let sig_pk_coords = sig_pubkey.to_affine().coordinates().unwrap();

    let mut public_inputs = vec![
        nullifier,
        *value_coords.x(),
        *value_coords.y(),
        *asset_coords.x(),
        *asset_coords.y(),
        // merkle_root,
        *sig_pk_coords.x(),
        *sig_pk_coords.y(),
    ];
}
