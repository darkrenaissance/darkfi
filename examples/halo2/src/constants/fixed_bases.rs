use super::{L_ORCHARD_SCALAR, L_VALUE};
use halo2_ecc::gadget::FixedPoints;

use arrayvec::ArrayVec;
use group::Curve;
use halo2::{
    arithmetic::{lagrange_interpolate, CurveAffine, Field, FieldExt},
    pasta::pallas,
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
pub const VALUE_COMMITMENT_V_BYTES: [u8; 1] = *b"v";

/// SWU hash-to-curve value for the value commitment generator
pub const VALUE_COMMITMENT_R_BYTES: [u8; 1] = *b"r";

/// Window size for fixed-base scalar multiplication
pub const FIXED_BASE_WINDOW_SIZE: usize = 3;

/// $2^{`FIXED_BASE_WINDOW_SIZE`}$
pub const H: usize = 1 << FIXED_BASE_WINDOW_SIZE;

/// Number of windows for a full-width scalar
pub const NUM_WINDOWS: usize =
    (L_ORCHARD_SCALAR + FIXED_BASE_WINDOW_SIZE - 1) / FIXED_BASE_WINDOW_SIZE;

/// Number of windows for a short signed scalar
pub const NUM_WINDOWS_SHORT: usize =
    (L_VALUE + FIXED_BASE_WINDOW_SIZE - 1) / FIXED_BASE_WINDOW_SIZE;

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
