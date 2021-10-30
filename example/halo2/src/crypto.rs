use pasta_curves::{
    arithmetic::{CurveExt, FieldExt},
    pallas,
};

use crate::constants::fixed_bases::{
    VALUE_COMMITMENT_PERSONALIZATION, VALUE_COMMITMENT_R_BYTES, VALUE_COMMITMENT_V_BYTES,
};

#[allow(non_snake_case)]
pub fn pedersen_commitment(value: u64, blind: pallas::Scalar) -> pallas::Point {
    let hasher = pallas::Point::hash_to_curve(VALUE_COMMITMENT_PERSONALIZATION);
    let V = hasher(&VALUE_COMMITMENT_V_BYTES);
    let R = hasher(&VALUE_COMMITMENT_R_BYTES);
    let value = pallas::Scalar::from_u64(value);

    V * value + R * blind
}
