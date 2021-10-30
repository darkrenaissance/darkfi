//! Type aliases used in the codebase.
// Helpful for changing the curve and crypto we're using.

pub type PublicKey = jubjub::SubgroupPoint;

pub type SecretKey = jubjub::Fr;

pub fn derive_publickey(secret: SecretKey) -> PublicKey {
    zcash_primitives::constants::SPENDING_KEY_GENERATOR * secret
}

pub type TokenId = jubjub::Fr;

pub type NullifierSerial = jubjub::Fr;

pub type CoinBlind = jubjub::Fr;

pub type ValueCommitBlind = jubjub::Fr;

pub type TokenCommitBlind = jubjub::Fr;
