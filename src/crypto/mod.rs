/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

pub mod burn_proof;
pub mod coin;
pub mod diffie_hellman;
pub mod mint_proof;
pub mod note;
pub mod types;

/// VDF (Verifiable Delay Function) using MiMC
pub mod mimc_vdf;

/// Halo2 proof API abstractions
pub mod proof;
pub use proof::Proof;

pub use burn_proof::BurnRevealedValues;
pub use mint_proof::MintRevealedValues;
