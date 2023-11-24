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

/// Contract ID definitions and methods
pub mod contract_id;
pub use contract_id::{ContractId, CONSENSUS_CONTRACT_ID, DAO_CONTRACT_ID, MONEY_CONTRACT_ID};

/// Token ID definitions and methods
pub mod token_id;
pub use token_id::{TokenId, DARK_TOKEN_ID};

/// Merkle node definitions
pub mod merkle_node;
pub use merkle_node::{MerkleNode, MerkleTree};

/// Note encryption
pub mod note;

/// Nullifier definitions
pub mod nullifier;
pub use nullifier::Nullifier;

/// Pedersen commitment utilities
pub mod pedersen;
pub use pedersen::{pedersen_commitment_base, pedersen_commitment_u64};

/// Schnorr signature traits
pub mod schnorr;

/// MiMC VDF
pub mod mimc_vdf;

/// Elliptic curve VRF (Verifiable Random Function)
pub mod ecvrf;

/// Sparse Merkle Tree implementation
pub mod smt;

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

/// Wallet Import Format
pub mod wif;
pub use wif::Wif;

#[macro_export]
macro_rules! fp_from_bs58 {
    ($ty:ident) => {
        impl FromStr for $ty {
            type Err = ContractError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let bytes = match bs58::decode(s).into_vec() {
                    Ok(v) => v,
                    Err(e) => return Err(ContractError::IoError(e.to_string())),
                };

                if bytes.len() != 32 {
                    return Err(ContractError::IoError(
                        "Length of decoded bytes is not 32".to_string(),
                    ))
                }

                Self::from_bytes(bytes.try_into().unwrap())
            }
        }
    };
}

#[macro_export]
macro_rules! fp_to_bs58 {
    ($ty:ident) => {
        impl std::fmt::Display for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "{}", bs58::encode(self.to_bytes()).into_string())
            }
        }
    };
}

#[macro_export]
macro_rules! ty_from_fp {
    ($ty:ident) => {
        impl From<pallas::Base> for $ty {
            fn from(x: pallas::Base) -> Self {
                Self(x)
            }
        }
    };
}
