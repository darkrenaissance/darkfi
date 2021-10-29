use halo2::arithmetic::{CurveAffine, FieldExt};
use halo2_gadgets::sinsemilla::{CommitDomains, HashDomains};
use pasta_curves::pallas;

use crate::constants::OrchardFixedBases;

/// Generator used in SinsemillaHashToPoint for note commitment
pub const Q_NOTE_COMMITMENT_M_GENERATOR: ([u8; 32], [u8; 32]) = (
    [
        93, 116, 168, 64, 9, 186, 14, 50, 42, 221, 70, 253, 90, 15, 150, 197, 93, 237, 176, 121,
        180, 242, 159, 247, 13, 205, 251, 86, 160, 7, 128, 23,
    ],
    [
        99, 172, 73, 115, 90, 10, 39, 135, 158, 94, 219, 129, 136, 18, 34, 136, 44, 201, 244, 110,
        217, 194, 190, 78, 131, 112, 198, 138, 147, 88, 160, 50,
    ],
);

/// Generator used in SinsemillaHashToPoint for IVK commitment
pub const Q_COMMIT_IVK_M_GENERATOR: ([u8; 32], [u8; 32]) = (
    [
        242, 130, 15, 121, 146, 47, 203, 107, 50, 162, 40, 81, 36, 204, 27, 66, 250, 65, 162, 90,
        184, 129, 204, 125, 17, 200, 169, 74, 241, 12, 188, 5,
    ],
    [
        190, 222, 173, 207, 206, 229, 90, 190, 241, 165, 109, 201, 29, 53, 196, 70, 75, 5, 222, 32,
        70, 7, 89, 239, 230, 190, 26, 212, 246, 76, 1, 27,
    ],
);

/// Generator used in SinsemillaHashToPoint for Merkle collision-resistant hash
pub const Q_MERKLE_CRH: ([u8; 32], [u8; 32]) = (
    [
        160, 198, 41, 127, 249, 199, 185, 248, 112, 16, 141, 192, 85, 185, 190, 201, 153, 14, 137,
        239, 90, 54, 15, 160, 185, 24, 168, 99, 150, 210, 22, 22,
    ],
    [
        98, 234, 242, 37, 206, 174, 233, 134, 150, 21, 116, 5, 234, 150, 28, 226, 121, 89, 163, 79,
        62, 242, 196, 45, 153, 32, 175, 227, 163, 66, 134, 53,
    ],
);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrchardHashDomains {
    NoteCommit,
    CommitIvk,
    MerkleCrh,
}

#[allow(non_snake_case)]
impl HashDomains<pallas::Affine> for OrchardHashDomains {
    fn Q(&self) -> pallas::Affine {
        match self {
            OrchardHashDomains::CommitIvk => pallas::Affine::from_xy(
                pallas::Base::from_bytes(&Q_COMMIT_IVK_M_GENERATOR.0).unwrap(),
                pallas::Base::from_bytes(&Q_COMMIT_IVK_M_GENERATOR.1).unwrap(),
            )
            .unwrap(),
            OrchardHashDomains::NoteCommit => pallas::Affine::from_xy(
                pallas::Base::from_bytes(&Q_NOTE_COMMITMENT_M_GENERATOR.0).unwrap(),
                pallas::Base::from_bytes(&Q_NOTE_COMMITMENT_M_GENERATOR.1).unwrap(),
            )
            .unwrap(),
            OrchardHashDomains::MerkleCrh => pallas::Affine::from_xy(
                pallas::Base::from_bytes(&Q_MERKLE_CRH.0).unwrap(),
                pallas::Base::from_bytes(&Q_MERKLE_CRH.1).unwrap(),
            )
            .unwrap(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrchardCommitDomains {
    NoteCommit,
    CommitIvk,
}

impl CommitDomains<pallas::Affine, OrchardFixedBases, OrchardHashDomains> for OrchardCommitDomains {
    fn r(&self) -> OrchardFixedBases {
        match self {
            Self::NoteCommit => OrchardFixedBases::NoteCommitR,
            Self::CommitIvk => OrchardFixedBases::CommitIvkR,
        }
    }

    fn hash_domain(&self) -> OrchardHashDomains {
        match self {
            Self::NoteCommit => OrchardHashDomains::NoteCommit,
            Self::CommitIvk => OrchardHashDomains::CommitIvk,
        }
    }
}
