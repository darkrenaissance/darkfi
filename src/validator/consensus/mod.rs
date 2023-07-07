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

use crate::{blockchain::Blockchain, util::time::TimeKeeper};

/// DarkFi consensus PID controller
pub mod pid;

/// Base 10 big float implementation for high precision arithmetics
pub mod float_10;

/// This struct represents the information required by the consensus algorithm
pub struct Consensus {
    /// Canonical (finalized) blockchain
    pub blockchain: Blockchain,
    /// Helper structure to calculate time related operations
    pub time_keeper: TimeKeeper,
}

impl Consensus {
    /// Generate a new Consensus state.
    pub fn new(blockchain: Blockchain, time_keeper: TimeKeeper) -> Self {
        Self { blockchain, time_keeper }
    }
}
