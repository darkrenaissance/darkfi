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

use darkfi_sdk::{
    crypto::{Nullifier, SecretKey},
    pasta::{group::ff::PrimeField, pallas},
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use super::note::Note;

#[derive(Clone, Copy, PartialEq, Eq, Debug, SerialEncodable, SerialDecodable)]
pub struct Coin(pub pallas::Base);

impl Coin {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        pallas::Base::from_repr(bytes).map(Coin).unwrap()
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_repr()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct OwnCoin {
    pub coin: Coin,
    pub note: Note,
    pub secret: SecretKey,
    pub nullifier: Nullifier,
    pub leaf_position: incrementalmerkletree::Position,
}
