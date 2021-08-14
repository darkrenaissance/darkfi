//! Constants used in the Orchard protocol.
use arrayvec::ArrayVec;
use ff::{Field, PrimeField};
use group::Curve;
use halo2::arithmetic::lagrange_interpolate;
use pasta_curves::{
    arithmetic::{CurveAffine, FieldExt},
    pallas,
};

pub mod commit_ivk_r;
pub mod note_commit_r;
pub mod nullifier_k;
pub mod spend_auth_g;
pub mod value_commit_r;
pub mod value_commit_v;

pub mod load;
pub mod util;

pub use load::{NullifierK, OrchardFixedBase, OrchardFixedBasesFull, ValueCommitV};

/// The Pallas scalar field modulus is $q = 2^{254} + \mathsf{t_q}$.
/// <https://github.com/zcash/pasta>
pub(crate) const T_Q: u128 = 45560315531506369815346746415080538113;

/// The Pallas base field modulus is $p = 2^{254} + \mathsf{t_p}$.
/// <https://github.com/zcash/pasta>
pub(crate) const T_P: u128 = 45560315531419706090280762371685220353;

/// $\mathsf{MerkleDepth^{Orchard}}$
pub(crate) const MERKLE_DEPTH_ORCHARD: usize = 32;

/// $\ell^\mathsf{Orchard}_\mathsf{Merkle}$
pub(crate) const L_ORCHARD_MERKLE: usize = 255;

/// $\ell^\mathsf{Orchard}_\mathsf{base}$
pub(crate) const L_ORCHARD_BASE: usize = 255;

/// $\ell^\mathsf{Orchard}_\mathsf{scalar}$
pub(crate) const L_ORCHARD_SCALAR: usize = 255;

/// $\ell_\mathsf{value}$
pub(crate) const L_VALUE: usize = 64;

/// T_PRIME_BITS is the smallest multiple of 10 such that 2^T_PRIME_BITS
/// is larger than t_P. t_P is defined in q_P = 2^254 + t_P for the
/// Pallas base field.
pub(crate) const PALLAS_T_PRIME_BITS: usize = 130;

/// SWU hash-to-curve personalization for the spending key base point and
/// the nullifier base point K^Orchard
pub const ORCHARD_PERSONALIZATION: &str = "z.cash:Orchard";

/// SWU hash-to-curve personalization for the group hash for key diversification
pub const KEY_DIVERSIFICATION_PERSONALIZATION: &str = "z.cash:Orchard-gd";

/// SWU hash-to-curve personalization for the value commitment generator
pub const VALUE_COMMITMENT_PERSONALIZATION: &str = "z.cash:Orchard-cv";

/// SWU hash-to-curve value for the value commitment generator
pub const VALUE_COMMITMENT_V_BYTES: [u8; 1] = *b"v";

/// SWU hash-to-curve value for the value commitment generator
pub const VALUE_COMMITMENT_R_BYTES: [u8; 1] = *b"r";

/// SWU hash-to-curve personalization for the note commitment generator
pub const NOTE_COMMITMENT_PERSONALIZATION: &str = "z.cash:Orchard-NoteCommit";

/// SWU hash-to-curve personalization for the IVK commitment generator
pub const COMMIT_IVK_PERSONALIZATION: &str = "z.cash:Orchard-CommitIvk";

/// SWU hash-to-curve personalization for the Merkle CRH generator
pub const MERKLE_CRH_PERSONALIZATION: &str = "z.cash:Orchard-MerkleCRH";

/// Window size for fixed-base scalar multiplication
pub const FIXED_BASE_WINDOW_SIZE: usize = 3;

/// $2^{`FIXED_BASE_WINDOW_SIZE`}$
pub const H: usize = 1 << FIXED_BASE_WINDOW_SIZE;

/// Number of windows for a full-width scalar
pub const NUM_WINDOWS: usize =
    (pallas::Base::NUM_BITS as usize + FIXED_BASE_WINDOW_SIZE - 1) / FIXED_BASE_WINDOW_SIZE;

/// Number of windows for a short signed scalar
pub const NUM_WINDOWS_SHORT: usize =
    (L_VALUE + FIXED_BASE_WINDOW_SIZE - 1) / FIXED_BASE_WINDOW_SIZE;

/// For each fixed base, we calculate its scalar multiples in three-bit windows.
/// Each window will have $2^3 = 8$ points.
fn compute_window_table<C: CurveAffine>(base: C, num_windows: usize) -> Vec<[C; H]> {
    let mut window_table: Vec<[C; H]> = Vec::with_capacity(num_windows);

    // Generate window table entries for all windows but the last.
    // For these first `num_windows - 1` windows, we compute the multiple [(k+2)*(2^3)^w]B.
    // Here, w ranges from [0..`num_windows - 1`)
    for w in 0..(num_windows - 1) {
        window_table.push(
            (0..H)
                .map(|k| {
                    // scalar = (k+2)*(8^w)
                    let scalar = C::ScalarExt::from_u64(k as u64 + 2)
                        * C::ScalarExt::from_u64(H as u64).pow(&[w as u64, 0, 0, 0]);
                    (base * scalar).to_affine()
                })
                .collect::<ArrayVec<C, H>>()
                .into_inner()
                .unwrap(),
        );
    }

    // Generate window table entries for the last window, w = `num_windows - 1`.
    // For the last window, we compute [k * (2^3)^w - sum]B, where sum is defined
    // as sum = \sum_{j = 0}^{`num_windows - 2`} 2^{3j+1}
    let sum = (0..(num_windows - 1)).fold(C::ScalarExt::zero(), |acc, j| {
        acc + C::ScalarExt::from_u64(2).pow(&[
            FIXED_BASE_WINDOW_SIZE as u64 * j as u64 + 1,
            0,
            0,
            0,
        ])
    });
    window_table.push(
        (0..H)
            .map(|k| {
                // scalar = k * (2^3)^w - sum, where w = `num_windows - 1`
                let scalar = C::ScalarExt::from_u64(k as u64)
                    * C::ScalarExt::from_u64(H as u64).pow(&[(num_windows - 1) as u64, 0, 0, 0])
                    - sum;
                (base * scalar).to_affine()
            })
            .collect::<ArrayVec<C, H>>()
            .into_inner()
            .unwrap(),
    );

    window_table
}

/// For each window, we interpolate the $x$-coordinate.
/// Here, we pre-compute and store the coefficients of the interpolation polynomial.
fn compute_lagrange_coeffs<C: CurveAffine>(base: C, num_windows: usize) -> Vec<[C::Base; H]> {
    // We are interpolating over the 3-bit window, k \in [0..8)
    let points: Vec<_> = (0..H).map(|i| C::Base::from_u64(i as u64)).collect();

    let window_table = compute_window_table(base, num_windows);

    window_table
        .iter()
        .map(|window_points| {
            let x_window_points: Vec<_> = window_points
                .iter()
                .map(|point| *point.coordinates().unwrap().x())
                .collect();
            lagrange_interpolate(&points, &x_window_points)
                .into_iter()
                .collect::<ArrayVec<C::Base, H>>()
                .into_inner()
                .unwrap()
        })
        .collect()
}

/// For each window, $z$ is a field element such that for each point $(x, y)$ in the window:
/// - $z + y = u^2$ (some square in the field); and
/// - $z - y$ is not a square.
/// If successful, return a vector of `(z: u64, us: [C::Base; H])` for each window.
///
/// This function was used to generate the `z`s and `u`s for the Orchard fixed
/// bases. The outputs of this function have been stored as constants, and it
/// is not called anywhere in this codebase. However, we keep this function here
/// as a utility for those who wish to use it with different parameters.
fn find_zs_and_us<C: CurveAffine>(base: C, num_windows: usize) -> Option<Vec<(u64, [C::Base; H])>> {
    // Closure to find z and u's for one window
    let find_z_and_us = |window_points: &[C]| {
        assert_eq!(H, window_points.len());

        let ys: Vec<_> = window_points
            .iter()
            .map(|point| *point.coordinates().unwrap().y())
            .collect();
        (0..(1000 * (1 << (2 * H)))).find_map(|z| {
            ys.iter()
                .map(|&y| {
                    if (-y + C::Base::from_u64(z)).sqrt().is_none().into() {
                        (y + C::Base::from_u64(z)).sqrt().into()
                    } else {
                        None
                    }
                })
                .collect::<Option<ArrayVec<C::Base, H>>>()
                .map(|us| (z, us.into_inner().unwrap()))
        })
    };

    let window_table = compute_window_table(base, num_windows);
    window_table
        .iter()
        .map(|window_points| find_z_and_us(window_points))
        .collect()
}

#[cfg(test)]
// Test that Lagrange interpolation coefficients reproduce the correct x-coordinate
// for each fixed-base multiple in each window.
fn test_lagrange_coeffs<C: CurveAffine>(base: C, num_windows: usize) {
    let lagrange_coeffs = compute_lagrange_coeffs(base, num_windows);

    // Check first 84 windows, i.e. `k_0, k_1, ..., k_83`
    for (idx, coeffs) in lagrange_coeffs[0..(num_windows - 1)].iter().enumerate() {
        // Test each three-bit chunk in this window.
        for bits in 0..(1 << FIXED_BASE_WINDOW_SIZE) {
            {
                // Interpolate the x-coordinate using this window's coefficients
                let interpolated_x = util::evaluate::<C>(bits, coeffs);

                // Compute the actual x-coordinate of the multiple [(k+2)*(8^w)]B.
                let point = base
                    * C::Scalar::from_u64(bits as u64 + 2)
                    * C::Scalar::from_u64(H as u64).pow(&[idx as u64, 0, 0, 0]);
                let x = *point.to_affine().coordinates().unwrap().x();

                // Check that the interpolated x-coordinate matches the actual one.
                assert_eq!(x, interpolated_x);
            }
        }
    }

    // Check last window.
    for bits in 0..(1 << FIXED_BASE_WINDOW_SIZE) {
        // Interpolate the x-coordinate using the last window's coefficients
        let interpolated_x = util::evaluate::<C>(bits, &lagrange_coeffs[num_windows - 1]);

        // Compute the actual x-coordinate of the multiple [k * (8^84) - offset]B,
        // where offset = \sum_{j = 0}^{83} 2^{3j+1}
        let offset = (0..(num_windows - 1)).fold(C::Scalar::zero(), |acc, w| {
            acc + C::Scalar::from_u64(2).pow(&[
                FIXED_BASE_WINDOW_SIZE as u64 * w as u64 + 1,
                0,
                0,
                0,
            ])
        });
        let scalar = C::Scalar::from_u64(bits as u64)
            * C::Scalar::from_u64(H as u64).pow(&[(num_windows - 1) as u64, 0, 0, 0])
            - offset;
        let point = base * scalar;
        let x = *point.to_affine().coordinates().unwrap().x();

        // Check that the interpolated x-coordinate matches the actual one.
        assert_eq!(x, interpolated_x);
    }
}

#[cfg(test)]
// Test that the z-values and u-values satisfy the conditions:
//      1. z + y = u^2,
//      2. z - y is not a square
// for the y-coordinate of each fixed-base multiple in each window.
fn test_zs_and_us<C: CurveAffine>(base: C, z: &[u64], u: &[[[u8; 32]; H]], num_windows: usize) {
    let window_table = compute_window_table(base, num_windows);

    for ((u, z), window_points) in u.iter().zip(z.iter()).zip(window_table) {
        for (u, point) in u.iter().zip(window_points.iter()) {
            let y = *point.coordinates().unwrap().y();
            let u = C::Base::from_bytes(u).unwrap();
            assert_eq!(C::Base::from_u64(*z) + y, u * u); // allow either square root
            assert!(bool::from((C::Base::from_u64(*z) - y).sqrt().is_none()));
        }
    }
}

#[cfg(test)]
mod tests {
    use ff::PrimeField;
    use pasta_curves::{arithmetic::FieldExt, pallas};

    #[test]
    // Nodes in the Merkle tree are Pallas base field elements.
    fn l_orchard_merkle() {
        assert_eq!(super::L_ORCHARD_MERKLE, pallas::Base::NUM_BITS as usize);
    }

    #[test]
    // Orchard uses the Pallas base field as its base field.
    fn l_orchard_base() {
        assert_eq!(super::L_ORCHARD_BASE, pallas::Base::NUM_BITS as usize);
    }

    #[test]
    // Orchard uses the Pallas base field as its base field.
    fn l_orchard_scalar() {
        assert_eq!(super::L_ORCHARD_SCALAR, pallas::Scalar::NUM_BITS as usize);
    }

    #[test]
    fn t_q() {
        let t_q = pallas::Scalar::from_u128(super::T_Q);
        let two_pow_254 = pallas::Scalar::from_u128(1 << 127).square();
        assert_eq!(t_q + two_pow_254, pallas::Scalar::zero());
    }

    #[test]
    fn t_p() {
        let t_p = pallas::Base::from_u128(super::T_P);
        let two_pow_254 = pallas::Base::from_u128(1 << 127).square();
        assert_eq!(t_p + two_pow_254, pallas::Base::zero());
    }
}
