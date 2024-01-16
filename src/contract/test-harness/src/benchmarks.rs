/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use log::info;
use std::time::Duration;

use crate::TxAction;

/// Auxiliary struct to calculate transaction actions benchmarks
pub struct TxActionBenchmarks {
    /// Vector holding each transaction size in Bytes
    pub sizes: Vec<usize>,
    /// Vector holding each transaction broadcasted size in Bytes
    pub broadcasted_sizes: Vec<usize>,
    /// Vector holding each transaction creation time
    pub creation_times: Vec<Duration>,
    /// Vector holding each transaction verify time
    pub verify_times: Vec<Duration>,
}

impl TxActionBenchmarks {
    pub fn new() -> Self {
        Self {
            sizes: vec![],
            broadcasted_sizes: vec![],
            creation_times: vec![],
            verify_times: vec![],
        }
    }

    pub fn statistics(&self, action: &TxAction) {
        if !self.sizes.is_empty() {
            let avg = self.sizes.iter().sum::<usize>();
            let avg = avg / self.sizes.len();
            info!("Average {:?} size: {:?} Bytes", action, avg);
        }
        if !self.broadcasted_sizes.is_empty() {
            let avg = self.broadcasted_sizes.iter().sum::<usize>();
            let avg = avg / self.broadcasted_sizes.len();
            info!("Average {:?} broadcasted size: {:?} Bytes", action, avg);
        }
        if !self.creation_times.is_empty() {
            let avg = self.creation_times.iter().sum::<Duration>();
            let avg = avg / self.creation_times.len() as u32;
            info!("Average {:?} creation time: {:?}", action, avg);
        }
        if !self.verify_times.is_empty() {
            let avg = self.verify_times.iter().sum::<Duration>();
            let avg = avg / self.verify_times.len() as u32;
            info!("Average {:?} verification time: {:?}", action, avg);
        }
    }
}

impl Default for TxActionBenchmarks {
    fn default() -> Self {
        Self::new()
    }
}
