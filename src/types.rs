//! Type aliases used in the codebase.
// Helpful for changing the curve and crypto we're using.
use halo2_gadgets::ecc::FixedPoints;
use pasta_curves::pallas;

use crate::crypto::{constants::OrchardFixedBases, util::mod_r_p};

pub type DrkCircuitField = pallas::Base;

pub type DrkTokenId = pallas::Base;
pub type DrkSerial = pallas::Base;

pub type DrkCoin = pallas::Base;
pub type DrkCoinBlind = pallas::Base;

pub type DrkNullifier = pallas::Base;

pub type DrkValue = pallas::Base;
pub type DrkScalar = pallas::Scalar;
pub type DrkValueBlind = pallas::Scalar;
pub type DrkValueCommit = pallas::Point;

pub type DrkPublicKey = pallas::Point;
pub type DrkSecretKey = pallas::Base;

pub fn derive_public_key(s: DrkSecretKey) -> DrkPublicKey {
    OrchardFixedBases::SpendAuthG.generator() * mod_r_p(s)
}
