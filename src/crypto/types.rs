//! Type aliases used in the codebase.
// Helpful for changing the curve and crypto we're using.
use halo2_gadgets::ecc::FixedPoints;
use pasta_curves as pasta;

use super::{constants::OrchardFixedBases, util::mod_r_p};

pub type DrkTokenId = pasta::Fp;
pub type DrkSerial = pasta::Fp;
pub type DrkCoinBlind = pasta::Fp;

pub type DrkValueBlind = pasta::Fq;
pub type DrkValueCommit = pasta::Ep;

pub type DrkPublicKey = pasta::Ep;
pub type DrkSecretKey = pasta::Fp;

pub fn derive_publickey(s: DrkSecretKey) -> DrkPublicKey {
    let skrt = mod_r_p(s);
    OrchardFixedBases::SpendAuthG.generator() * skrt
}
