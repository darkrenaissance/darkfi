use blake2b_simd::Params;
use pasta_curves as pasta;
use pasta_curves::{
    arithmetic::{CurveExt, FieldExt},
    group::ff::PrimeField,
};

use super::constants::fixed_bases::{
    VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_R_BYTES, VALUE_COMMITMENT_V_BYTES,
};

pub fn hash_to_scalar(persona: &[u8], a: &[u8], b: &[u8]) -> pasta::Fq {
    let mut hasher = Params::new().hash_length(64).personal(persona).to_state();
    hasher.update(a);
    hasher.update(b);
    let ret = hasher.finalize();
    pasta::Fq::from_bytes_wide(ret.as_array())
}

#[allow(non_snake_case)]
pub fn pedersen_commitment_scalar(value: pasta::Fq, blind: pasta::Fq) -> pasta::Ep {
    let hasher = pasta::Ep::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
    let V = hasher(&VALUE_COMMITMENT_V_BYTES);
    let R = hasher(&VALUE_COMMITMENT_R_BYTES);

    V * value + R * blind
}

pub fn pedersen_commitment_u64(value: u64, blind: pasta::Fq) -> pasta::Ep {
    pedersen_commitment_scalar(pasta::Fq::from_u64(value), blind)
}

pub fn mod_r_p(x: pasta::Fp) -> pasta::Fq {
    pasta::Fq::from_repr(x.to_repr()).unwrap()
}
