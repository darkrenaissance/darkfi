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

use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Parameters for `Consensus::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusStakeParamsV1 {
    // TODO: implement
}

/// State update for `Consensus::Stake`
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct ConsensusStakeUpdateV1 {
    // TODO: implement
}
