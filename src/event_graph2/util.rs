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

/// Calculate the timestamp of the next DAG rotation.
pub(super) fn next_rotation_timestamp(starting_timestamp: u64, rotation_period: u64) -> u64 {
    // Prevent division by 0
    if rotation_period == 0 {
        panic!("Rotation period cannot be 0");
    }
    // Calculate the number of days since the given starting point
    let days_passed = days_since(starting_timestamp);

    // Find out how many rotation periods have occurred since
    // the starting point
    let rotations_since_start = (days_passed + rotation_period - 1) / rotation_period;

    // Find out the number of days until the next rotation. Panic if result is beyond the range
    // of i64.
    let days_until_next_rotation: i64 =
        (rotations_since_start * rotation_period - days_passed).try_into().unwrap();

    // Get the timestamp for the next rotation
    midnight_timestamp(days_until_next_rotation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_since() {
        let five_days_ago = midnight_timestamp(-5);
        assert_eq!(days_since(five_days_ago), 5);

        let today = midnight_timestamp(0);
        assert_eq!(days_since(today), 0);
    }

    #[test]
    fn test_next_rotation_timestamp() {
        let starting_point = midnight_timestamp(-10);
        let rotation_period = 7;

        // The first rotation since the starting point would be 3 days ago.
        // So the next rotation should be 4 days from now.
        let expected = midnight_timestamp(4);
        assert_eq!(next_rotation_timestamp(starting_point, rotation_period), expected);
    }

    #[test]
    #[should_panic]
    fn test_next_rotation_timestamp_panics_on_overflow() {
        next_rotation_timestamp(0, u64::MAX);
    }

    #[test]
    #[should_panic]
    fn test_next_rotation_timestamp_panics_on_division_by_zero() {
        next_rotation_timestamp(0, 0);
    }
}
