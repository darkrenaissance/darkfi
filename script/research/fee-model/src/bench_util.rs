/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Helpers for `bench` and `db_bench` microbenchmarks.

use std::time::Instant;

/// Collect `iters` nanosecond timing samples for `op`.
pub fn collect_times<F: FnMut()>(mut op: F, iters: usize) -> Vec<u64> {
    let mut times = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        op();
        times.push(start.elapsed().as_nanos() as u64);
    }
    times
}

/// Minimal WASM module exporting `add: (i32, i32) -> i32`. Used as the
/// single-opcode baseline in both microbenchmarks.
pub const WASM_ADD: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, // Magic
    0x01, 0x00, 0x00, 0x00, // Version
    0x01, 0x07, 0x01, // Type section
    0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f, // (i32, i32) -> i32
    0x03, 0x02, 0x01, 0x00, // Function section
    0x07, 0x07, 0x01, 0x03, 0x61, 0x64, 0x64, 0x00, 0x00, // Export "add"
    0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
];
