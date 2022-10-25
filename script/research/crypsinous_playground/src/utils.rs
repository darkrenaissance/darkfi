use dashu::{
    float::{round::mode::Zero, FBig},
    integer::{IBig, Sign},
};
use log::{debug, info};
use pasta_curves::{group::ff::PrimeField, pallas};

pub type Float10 = FBig<Zero, 10>;

pub fn fbig2ibig(f: Float10) -> IBig {
    let rad = IBig::try_from(10).unwrap();
    let sig = f.repr().significand();
    let exp = f.repr().exponent();
    let val: IBig = if exp >= 0 {
        sig.clone() * rad.pow(exp as usize)
    } else {
        sig.clone()
    };
    debug!("fbig2ibig (f): {}", f);
    debug!("fbig2ibig (i): {}", val);
    val
}

pub fn fbig2base(f: Float10) -> pallas::Base {
    info!("fbig -> base (f): {}", f);
    let val: IBig = fbig2ibig(f);
    let (sign, word) = val.as_sign_words();
    //TODO (res) set pallas base sign, i.e sigma1 is negative.
    let mut words: [u64; 4] = [0, 0, 0, 0];
    for i in 0..word.len() {
        words[i] = word[i];
    }
    let base = match sign {
        Sign::Positive => pallas::Base::from_raw(words),
        Sign::Negative => pallas::Base::from_raw(words).neg(),
    };
    base
}

/// Extract leader selection lottery randomness(eta)
/// using the hash of the previous lead proof, converted to pallas base.
pub fn get_eta(proof_tx_hash: blake3::Hash) -> pallas::Base {
    let mut bytes: [u8; 32] = *proof_tx_hash.as_bytes();
    // read first 254 bits
    bytes[30] = 0;
    bytes[31] = 0;
    pallas::Base::from_repr(bytes).unwrap()
}
