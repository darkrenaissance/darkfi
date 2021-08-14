use ff::{Field, PrimeFieldBits};
use halo2::arithmetic::{CurveAffine, FieldExt};

/// Decompose a word `alpha` into `window_num_bits` bits (little-endian)
/// For a window size of `w`, this returns [k_0, ..., k_n] where each `k_i`
/// is a `w`-bit value, and `scalar = k_0 + k_1 * w + k_n * w^n`.
///
/// # Panics
///
/// We are returning a `Vec<u8>` which means the window size is limited to
/// <= 8 bits.
pub fn decompose_word<F: PrimeFieldBits>(
    word: F,
    word_num_bits: usize,
    window_num_bits: usize,
) -> Vec<u8> {
    assert!(window_num_bits <= 8);

    // Pad bits to multiple of window_num_bits
    let padding = (window_num_bits - (word_num_bits % window_num_bits)) % window_num_bits;
    let bits: Vec<bool> = word
        .to_le_bits()
        .into_iter()
        .take(word_num_bits)
        .chain(std::iter::repeat(false).take(padding))
        .collect();
    assert_eq!(bits.len(), word_num_bits + padding);

    bits.chunks_exact(window_num_bits)
        .map(|chunk| chunk.iter().rev().fold(0, |acc, b| (acc << 1) + (*b as u8)))
        .collect()
}

/// Evaluate y = f(x) given the coefficients of f(x)
pub fn evaluate<C: CurveAffine>(x: u8, coeffs: &[C::Base]) -> C::Base {
    let x = C::Base::from_u64(x as u64);
    coeffs
        .iter()
        .rev()
        .cloned()
        .reduce(|acc, coeff| acc * x + coeff)
        .unwrap_or_else(C::Base::zero)
}

/// Takes in an FnMut closure and returns a constant-length array with elements of
/// type `Output`.
pub fn gen_const_array<Output: Copy + Default, const LEN: usize>(
    mut closure: impl FnMut(usize) -> Output,
) -> [Output; LEN] {
    let mut ret: [Output; LEN] = [Default::default(); LEN];
    for (bit, val) in ret.iter_mut().zip((0..LEN).map(|idx| closure(idx))) {
        *bit = val;
    }
    ret
}

#[cfg(test)]
mod tests {
    use super::decompose_word;
    use ff::PrimeField;
    use pasta_curves::{arithmetic::FieldExt, pallas};
    use proptest::prelude::*;
    use std::convert::TryInto;
    use std::iter;

    prop_compose! {
        fn arb_scalar()(bytes in prop::array::uniform32(0u8..)) -> pallas::Scalar {
            // Instead of rejecting out-of-range bytes, let's reduce them.
            let mut buf = [0; 64];
            buf[..32].copy_from_slice(&bytes);
            pallas::Scalar::from_bytes_wide(&buf)
        }
    }

    proptest! {
        #[test]
        fn test_decompose_word(
            scalar in arb_scalar(),
            window_num_bits in 1u8..9
        ) {
            // Get decomposition into `window_num_bits` bits
            let decomposed = decompose_word(scalar, pallas::Scalar::NUM_BITS as usize, window_num_bits as usize);

            // Flatten bits
            let bits = decomposed
                .iter()
                .flat_map(|window| (0..window_num_bits).map(move |mask| (window & (1 << mask)) != 0));

            // Ensure this decomposition contains 256 or fewer set bits.
            assert!(!bits.clone().skip(32*8).any(|b| b));

            // Pad or truncate bits to 32 bytes
            let bits: Vec<bool> = bits.chain(iter::repeat(false)).take(32*8).collect();

            let bytes: Vec<u8> = bits.chunks_exact(8).map(|chunk| chunk.iter().rev().fold(0, |acc, b| (acc << 1) + (*b as u8))).collect();

            // Check that original scalar is recovered from decomposition
            assert_eq!(scalar, pallas::Scalar::from_bytes(&bytes.try_into().unwrap()).unwrap());
        }
    }
}
