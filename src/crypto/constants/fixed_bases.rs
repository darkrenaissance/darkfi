use halo2_gadgets::ecc::{
    chip::{compute_lagrange_coeffs, NUM_WINDOWS, NUM_WINDOWS_SHORT},
    FixedPoints, H,
};
use pasta_curves::pallas;

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
