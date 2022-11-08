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

//! Type aliases used in the codebase.
// Helpful for changing the curve and crypto we're using.
use pasta_curves::pallas;

pub type DrkCircuitField = pallas::Base;

pub type DrkValue = pallas::Base;
pub type DrkSerial = pallas::Base;

pub type DrkSpendHook = pallas::Base;
pub type DrkUserData = pallas::Base;
pub type DrkUserDataBlind = pallas::Base;
pub type DrkUserDataEnc = pallas::Base;

pub type DrkCoinBlind = pallas::Base;
pub type DrkValueBlind = pallas::Scalar;
pub type DrkValueCommit = pallas::Point;
