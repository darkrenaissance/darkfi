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

/// epoch configuration
/// this struct need be a singleton,
/// TODO should be populated from configuration file.
#[derive(Copy, Debug, Default, Clone)]
pub struct EpochConsensus {
    pub sl_len: u64, // length of slot in terms of ticks
    // number of slots per epoch
    pub e_len: u64,    // length of epoch in terms of slots
    pub tick_len: u64, // length of tick in terms of seconds
    pub reward: u64,   // constant reward value for the slot leader
}

impl EpochConsensus {
    pub fn new(
        sl_len: Option<u64>,
        e_len: Option<u64>,
        tick_len: Option<u64>,
        reward: Option<u64>,
    ) -> Self {
        Self {
            sl_len: sl_len.unwrap_or(22),
            e_len: e_len.unwrap_or(3),
            tick_len: tick_len.unwrap_or(22),
            reward: reward.unwrap_or(1),
        }
    }

    pub fn total_stake(&self, e: u64, sl: u64) -> u64 {
        (e * self.e_len + sl + 1) * self.reward
    }
    /// getter for constant stakeholder reward
    /// used for configuring the stakeholder reward value
    pub fn get_reward(&self) -> u64 {
        self.reward
    }

    /// getter for the slot length in terms of ticks
    pub fn get_slot_len(&self) -> u64 {
        self.sl_len
    }

    /// getter for the epoch length in terms of slots
    pub fn get_epoch_len(&self) -> u64 {
        self.e_len
    }

    /// getter for the ticks length in terms of seconds
    pub fn get_tick_len(&self) -> u64 {
        self.tick_len
    }
}
