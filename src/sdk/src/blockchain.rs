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

/// Auxiliary function to calculate provided block height block version.
/// Currently, a single version(1) exists.
pub fn block_version(_height: u32) -> u8 {
    1
}

/// Auxiliary function to calculate provided block height epoch.
/// Each epoch is defined by the fixed intervals rewards change.
/// Genesis block is on epoch 0.
pub fn block_epoch(height: u32) -> u8 {
    match height {
        0 => 0,
        1..=1000 => 1,
        1001..=2000 => 2,
        2001..=3000 => 3,
        3001..=4000 => 4,
        4001..=5000 => 5,
        5001..=6000 => 6,
        6001..=7000 => 7,
        7001..=8000 => 8,
        8001..=9000 => 9,
        9001..=10000 => 10,
        10001.. => 11,
    }
}

/// Auxiliary function to calculate provided block height expected reward value.
///
/// Genesis block always returns reward value 0. Rewards are halfed at fixed intervals,
/// called epochs. After last epoch has started, reward value is based on DARK token-economics.
pub fn expected_reward(height: u32) -> u64 {
    // Grab block height epoch
    let epoch = block_epoch(height);

    // TODO (res) implement reward mechanism with accord to DRK, DARK token-economics.
    // Configured block rewards (1 DRK == 1 * 10^8)
    match epoch {
        0 => 0,
        1 => 2_000_000_000, // 20 DRK
        2 => 1_800_000_000, // 18 DRK
        3 => 1_600_000_000, // 16 DRK
        4 => 1_400_000_000, // 14 DRK
        5 => 1_200_000_000, // 12 DRK
        6 => 1_000_000_000, // 10 DRK
        7 => 800_000_000,   // 8 DRK
        8 => 600_000_000,   // 6 DRK
        9 => 400_000_000,   // 4 DRK
        10 => 200_000_000,  // 2 DRK
        _ => 100_000_000,   // 1 DRK
    }
}
