//! Type aliases used in the codebase.
// Helpful for changing the curve and crypto we're using.
use halo2_gadgets::ecc::FixedPoints;
use pasta_curves as pasta;

use super::{constants::OrchardFixedBases, util::mod_r_p};

pub type DrkCircuitField = pasta::Fp;

pub type DrkTokenId = pasta::Fp;
pub type DrkSerial = pasta::Fp;

pub type DrkCoin = pasta::Fp;
pub type DrkCoinBlind = pasta::Fp;

pub type DrkNullifier = pasta::Fp;

pub type DrkValue = pasta::Fp;
pub type DrkScalar = pasta::Fq;
pub type DrkValueBlind = pasta::Fq;
pub type DrkValueCommit = pasta::Ep;

pub type DrkPublicKey = pasta::Ep;
pub type DrkSecretKey = pasta::Fp;

pub fn derive_publickey(s: DrkSecretKey) -> DrkPublicKey {
    OrchardFixedBases::SpendAuthG.generator() * mod_r_p(s)
}
