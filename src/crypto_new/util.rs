use crate::crypto_new::types::*;

#[allow(non_snake_case)]
pub fn pedersen_commitment_scalar(value: DrkValue, blind: DrkValueBlind) -> DrkValueCommit {
    let V = zcash_primitives::constants::VALUE_COMMITMENT_VALUE_GENERATOR;
    let R = zcash_primitives::constants::VALUE_COMMITMENT_RANDOMNESS_GENERATOR;

    V * value + R * blind
}

pub fn pedersen_commitment_u64(value: u64, blind: DrkValueBlind) -> DrkValueCommit {
    pedersen_commitment_scalar(DrkValue::from(value), blind)
}
