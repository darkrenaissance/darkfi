use arrayvec::ArrayVec;
use halo2_gadgets::ecc::{
    chip::{compute_lagrange_coeffs, NUM_WINDOWS, NUM_WINDOWS_SHORT},
    FixedPoints, H,
};
use pasta_curves::pallas;
use pasta_curves::{
    arithmetic::{CurveAffine, Field, FieldExt},
    group::Curve,
};

pub mod commit_ivk_r;
pub mod note_commit_r;
pub mod nullifier_k;
pub mod spend_auth_g;
pub mod value_commit_r;
pub mod value_commit_v;

/// SWU hash-to-curve personalization for the value commitment generator
pub const VALUE_COMMITMENT_PERSONALIZATION: &str = "z.cash:Orchard-cv";

/// SWU hash-to-curve value for the value commitment generator
pub const VALUE_COMMITMENT_R_BYTES: [u8; 1] = *b"r";

/// SWU hash-to-curve value for the value commitment generator
pub const VALUE_COMMITMENT_V_BYTES: [u8; 1] = *b"v";

/// SWU hash-to-curve personalization for the note commitment generator
pub const NOTE_COMMITMENT_PERSONALIZATION: &str = "z.cash:Orchard-NoteCommit";

/// SWU hash-to-curve personalization for the IVK commitment generator
pub const COMMIT_IVK_PERSONALIZATION: &str = "z.cash:Orchard-CommitIvk";

/// SWU hash-to-curve personalization for the spending key base point and
/// the nullifier base point K^Orchard
pub const ORCHARD_PERSONALIZATION: &str = "z.cash:Orchard";

/// Window size for fixed-base scalar multiplication
pub const FIXED_BASE_WINDOW_SIZE: usize = 3;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum OrchardFixedBases {
    CommitIvkR,
    NoteCommitR,
    ValueCommitR,
    SpendAuthG,
    NullifierK,
    ValueCommitV,
}

impl FixedPoints<pallas::Affine> for OrchardFixedBases {
    fn generator(&self) -> pallas::Affine {
        match self {
            OrchardFixedBases::CommitIvkR => commit_ivk_r::generator(),
            OrchardFixedBases::NoteCommitR => note_commit_r::generator(),
            OrchardFixedBases::ValueCommitR => value_commit_r::generator(),
            OrchardFixedBases::SpendAuthG => spend_auth_g::generator(),
            OrchardFixedBases::NullifierK => nullifier_k::generator(),
            OrchardFixedBases::ValueCommitV => value_commit_v::generator(),
        }
    }
    fn u(&self) -> Vec<[[u8; 32]; H]> {
        match self {
            OrchardFixedBases::CommitIvkR => commit_ivk_r::U.to_vec(),
            OrchardFixedBases::NoteCommitR => note_commit_r::U.to_vec(),
            OrchardFixedBases::ValueCommitR => value_commit_r::U.to_vec(),
            OrchardFixedBases::SpendAuthG => spend_auth_g::U.to_vec(),
            OrchardFixedBases::NullifierK => nullifier_k::U.to_vec(),
            OrchardFixedBases::ValueCommitV => value_commit_v::U_SHORT.to_vec(),
        }
    }

    fn z(&self) -> Vec<u64> {
        match self {
            OrchardFixedBases::CommitIvkR => commit_ivk_r::Z.to_vec(),
            OrchardFixedBases::NoteCommitR => note_commit_r::Z.to_vec(),
            OrchardFixedBases::ValueCommitR => value_commit_r::Z.to_vec(),
            OrchardFixedBases::SpendAuthG => spend_auth_g::Z.to_vec(),
            OrchardFixedBases::NullifierK => nullifier_k::Z.to_vec(),
            OrchardFixedBases::ValueCommitV => value_commit_v::Z_SHORT.to_vec(),
        }
    }

    fn lagrange_coeffs(&self) -> Vec<[pallas::Base; H]> {
        match self {
            OrchardFixedBases::ValueCommitV => {
                compute_lagrange_coeffs(self.generator(), NUM_WINDOWS_SHORT)
            }
            _ => compute_lagrange_coeffs(self.generator(), NUM_WINDOWS),
        }
    }
}

/// For each fixed base, we calculate its scalar multiples in three-bit windows.
/// Each window will have $2^3 = 8$ points.
#[allow(dead_code)]
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
                let interpolated_x = super::util::evaluate::<C>(bits, coeffs);

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
        let interpolated_x = super::util::evaluate::<C>(bits, &lagrange_coeffs[num_windows - 1]);

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
