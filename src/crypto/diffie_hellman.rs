use blake2b_simd::{Hash as Blake2bHash, Params as Blake2bParams};
use group::{cofactor::CofactorGroup, GroupEncoding};

pub const KDF_SAPLING_PERSONALIZATION: &[u8; 16] = b"DarkFiSaplingKDF";

/// Functions used for encrypting the note in transaction outputs.

/// Sapling key agreement for note encryption.
///
/// Implements section 5.4.4.3 of the Zcash Protocol Specification.
pub fn sapling_ka_agree(esk: &jubjub::Fr, pk_d: &jubjub::ExtendedPoint) -> jubjub::SubgroupPoint {
    // [8 esk] pk_d
    // <ExtendedPoint as CofactorGroup>::clear_cofactor is implemented using
    // ExtendedPoint::mul_by_cofactor in the jubjub crate.

    let mut wnaf = group::Wnaf::new();
    wnaf.scalar(esk).base(*pk_d).clear_cofactor()
}

/// Sapling KDF for note encryption.
///
/// Implements section 5.4.4.4 of the Zcash Protocol Specification.
fn kdf_sapling(dhsecret: jubjub::SubgroupPoint, epk: &jubjub::ExtendedPoint) -> Blake2bHash {
    Blake2bParams::new()
        .hash_length(32)
        .personal(KDF_SAPLING_PERSONALIZATION)
        .to_state()
        .update(&dhsecret.to_bytes())
        .update(&epk.to_bytes())
        .finalize()
}

