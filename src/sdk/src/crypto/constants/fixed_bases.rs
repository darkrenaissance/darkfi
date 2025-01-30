/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Orchard fixed bases.
use super::{L_ORCHARD_SCALAR, L_VALUE};
use halo2_gadgets::ecc::{
    chip::{BaseFieldElem, FixedPoint, FullScalar, ShortScalar},
    FixedPoints,
};

use pasta_curves::pallas;

pub mod commit_ivk_r;
pub mod note_commit_r;
pub mod nullifier_k;
pub mod spend_auth_g;
pub mod value_commit_r;
pub mod value_commit_v;

/// SWU hash-to-curve personalization for the spending key base point and
/// the nullifier base point K^Orchard
pub const ORCHARD_PERSONALIZATION: &str = "z.cash:Orchard";

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

/// Window size for fixed-base scalar multiplication
pub const FIXED_BASE_WINDOW_SIZE: usize = 3;

/// $2^{`FIXED_BASE_WINDOW_SIZE`}$
pub const H: usize = 1 << FIXED_BASE_WINDOW_SIZE;

/// Number of windows for a full-width scalar
pub const NUM_WINDOWS: usize = L_ORCHARD_SCALAR.div_ceil(FIXED_BASE_WINDOW_SIZE);

/// Number of windows for a short signed scalar
pub const NUM_WINDOWS_SHORT: usize = L_VALUE.div_ceil(FIXED_BASE_WINDOW_SIZE);

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
// A sum type for both full-width and short bases. This enables us to use the
// shared functionality of full-width and short fixed-base scalar multiplication.
pub enum OrchardFixedBases {
    Full(OrchardFixedBasesFull),
    NullifierK,
    ValueCommitV,
}

impl From<OrchardFixedBasesFull> for OrchardFixedBases {
    fn from(full_width_base: OrchardFixedBasesFull) -> Self {
        Self::Full(full_width_base)
    }
}

impl From<ValueCommitV> for OrchardFixedBases {
    fn from(_value_commit_v: ValueCommitV) -> Self {
        Self::ValueCommitV
    }
}

impl From<NullifierK> for OrchardFixedBases {
    fn from(_nullifier_k: NullifierK) -> Self {
        Self::NullifierK
    }
}

/// The Orchard fixed bases used in scalar mul with full-width scalars.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum OrchardFixedBasesFull {
    CommitIvkR,
    NoteCommitR,
    ValueCommitR,
    SpendAuthG,
}

/// NullifierK is used in scalar mul with a base field element.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct NullifierK;

/// ValueCommitV is used in scalar mul with a short signed scalar.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ValueCommitV;

/// ConstBaseFieldElement is used in scalar mul with a base field element.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(non_snake_case)] // Rust bug: see https://github.com/rust-lang/rust/issues/60681
pub struct ConstBaseFieldElement {
    G: pallas::Affine,
    u: Vec<[[u8; 32]; H]>,
    z: Vec<u64>,
}

impl ConstBaseFieldElement {
    pub fn value_commit_r() -> Self {
        Self {
            G: value_commit_r::generator(),
            u: value_commit_r::U.to_vec(),
            z: value_commit_r::Z.to_vec(),
        }
    }
    pub fn nullifier_k() -> Self {
        Self { G: nullifier_k::generator(), u: nullifier_k::U.to_vec(), z: nullifier_k::Z.to_vec() }
    }
}

impl FixedPoints<pallas::Affine> for OrchardFixedBases {
    type FullScalar = OrchardFixedBasesFull;
    type ShortScalar = ValueCommitV;
    type Base = ConstBaseFieldElement;
}

impl FixedPoint<pallas::Affine> for OrchardFixedBasesFull {
    type FixedScalarKind = FullScalar;

    fn generator(&self) -> pallas::Affine {
        match self {
            Self::CommitIvkR => commit_ivk_r::generator(),
            Self::NoteCommitR => note_commit_r::generator(),
            Self::ValueCommitR => value_commit_r::generator(),
            Self::SpendAuthG => spend_auth_g::generator(),
        }
    }

    fn u(&self) -> Vec<[[u8; 32]; H]> {
        match self {
            Self::CommitIvkR => commit_ivk_r::U.to_vec(),
            Self::NoteCommitR => note_commit_r::U.to_vec(),
            Self::ValueCommitR => value_commit_r::U.to_vec(),
            Self::SpendAuthG => spend_auth_g::U.to_vec(),
        }
    }

    fn z(&self) -> Vec<u64> {
        match self {
            Self::CommitIvkR => commit_ivk_r::Z.to_vec(),
            Self::NoteCommitR => note_commit_r::Z.to_vec(),
            Self::ValueCommitR => value_commit_r::Z.to_vec(),
            Self::SpendAuthG => spend_auth_g::Z.to_vec(),
        }
    }
}

impl FixedPoint<pallas::Affine> for ConstBaseFieldElement {
    type FixedScalarKind = BaseFieldElem;

    fn generator(&self) -> pallas::Affine {
        self.G
    }

    fn u(&self) -> Vec<[[u8; 32]; H]> {
        self.u.clone()
    }

    fn z(&self) -> Vec<u64> {
        self.z.clone()
    }
}

impl FixedPoint<pallas::Affine> for NullifierK {
    type FixedScalarKind = BaseFieldElem;

    fn generator(&self) -> pallas::Affine {
        nullifier_k::generator()
    }

    fn u(&self) -> Vec<[[u8; 32]; H]> {
        nullifier_k::U.to_vec()
    }

    fn z(&self) -> Vec<u64> {
        nullifier_k::Z.to_vec()
    }
}

impl FixedPoint<pallas::Affine> for ValueCommitV {
    type FixedScalarKind = ShortScalar;

    fn generator(&self) -> pallas::Affine {
        value_commit_v::generator()
    }

    fn u(&self) -> Vec<[[u8; 32]; H]> {
        value_commit_v::U_SHORT.to_vec()
    }

    fn z(&self) -> Vec<u64> {
        value_commit_v::Z_SHORT.to_vec()
    }
}
