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

use std::time::UNIX_EPOCH;

/// Seconds in a day
pub(super) const DAY: i64 = 86400;

/// Calculate the midnight timestamp given a number of days.
/// If `days` is 0, calculate the midnight timestamp of today.
pub(super) fn midnight_timestamp(days: i64) -> u64 {
    // Get current time
    let now = UNIX_EPOCH.elapsed().unwrap().as_secs() as i64;

    // Find the timestamp for the midnight of the current day
    let cur_midnight = (now / DAY) * DAY;

    // Adjust for days_from_now
    (cur_midnight + (DAY * days)) as u64
}

/// Calculate the number of days since a given midnight timestamp.
pub(super) fn days_since(midnight_ts: u64) -> u64 {
    // Get current time
    let now = UNIX_EPOCH.elapsed().unwrap().as_secs();

    // Calculate the difference between the current timestamp
    // and the given midnight timestamp
    let elapsed_seconds = now - midnight_ts;

    // Convert the elapsed seconds into days
    elapsed_seconds / DAY as u64
}
