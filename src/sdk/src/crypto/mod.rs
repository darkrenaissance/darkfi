/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

//! This module contains a bit more minimal implementations of the
//! objects and types that can be found in `darkfi::crypto`.
//! This is done so we can have a lot less dependencies in this SDK,
//! and therefore make compilation of smart contracts faster in a sense.
//!
//! Eventually, we should strive to somehow migrate the types from
//! `darkfi::crypto` into here, and then implement certain functionality
//! in the library using traits.
//! If you feel like trying, please help out with this migration, but do
//! it properly, with care, and write documentation while you're at it.

/// Cryptographic constants
pub mod constants;

/// Diffie-Hellman techniques
pub mod diffie_hellman;

/// Miscellaneous utilities
pub mod util;
pub use util::poseidon_hash;

/// Keypairs, secret keys, and public keys
pub mod keypair;
pub use keypair::{Keypair, PublicKey, SecretKey};

/// Coin definitions and methods
pub mod coin;
pub use coin::Coin;

/// Contract ID definitions and methods
pub mod contract_id;
pub use contract_id::{ContractId, DAO_CONTRACT_ID, MONEY_CONTRACT_ID};

/// Token ID definitions and methods
pub mod token_id;
pub use token_id::{TokenId, DARK_TOKEN_ID};

/// Merkle node definitions
pub mod merkle_node;
pub use merkle_node::{MerkleNode, MerkleTree};

pub mod merkle_prelude {
    pub use incrementalmerkletree::{Hashable, Tree};
}
pub use incrementalmerkletree::Position as MerklePosition;

/// Note encryption
pub mod note;

/// Nullifier definitions
pub mod nullifier;
pub use nullifier::Nullifier;

/// Pedersen commitment utilities
pub mod pedersen;
pub use pedersen::{pedersen_commitment_base, pedersen_commitment_u64, ValueBlind, ValueCommit};

/// Schnorr signature traits
pub mod schnorr;

/// MiMC VDF
pub mod mimc_vdf;

pub use incrementalmerkletree;
pub use pasta_curves::{pallas, vesta};
/// Convenience module to import all the pasta traits.
/// You still have to import the curves.
pub mod pasta_prelude {
    pub use pasta_curves::{
        arithmetic::{CurveAffine, CurveExt},
        group::{
            ff::{Field, PrimeField},
            Curve, Group,
        },
    };
}
