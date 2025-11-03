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

/// Command-line interface utilities
pub mod cli;

/// Various encoding formats
pub mod encoding;

/// Filesystem utilities
pub mod file;

/// Parsing helpers
pub mod parse;

/// Filesystem path utilities
pub mod path;

/// Time utilities
pub mod time;

/// Ring Buffer implementation
pub mod ringbuffer;

/// Logging utilities
pub mod logger;

/// Permuted Congruential Generator (PCG)
/// This is an insecure PRNG used for simulations and tests.
#[cfg(feature = "rand")]
pub mod pcg;

/// Return the most frequent element in vec or just any item.
pub fn most_frequent_or_any<T: Eq + Clone>(items: &[T]) -> Option<T> {
    if items.is_empty() {
        return None;
    }

    let mut max_count = 0;
    let mut most_freq = &items[0];

    for i in 0..items.len() {
        let mut count = 0;

        for j in 0..items.len() {
            if items[i] == items[j] {
                count += 1;
            }
        }

        if count > max_count {
            max_count = count;
            most_freq = &items[i];
        }
    }

    Some(most_freq.clone())
}
