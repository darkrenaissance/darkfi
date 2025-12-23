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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuThread {
    affinity: i32,
    intensity: u32,
}

impl CpuThread {
    pub fn new(affinity: i32, intensity: Option<u32>) -> Self {
        Self { affinity, intensity: intensity.unwrap_or(0) }
    }

    pub fn is_valid(&self) -> bool {
        self.intensity <= 8
    }

    pub fn affinity(&self) -> i32 {
        self.affinity
    }

    pub fn intensity(&self) -> u32 {
        if self.intensity == 0 {
            1
        } else {
            self.intensity
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuThreads {
    affinity: i32,
    data: Vec<CpuThread>,
}

impl CpuThreads {
    pub fn new(count: usize, intensity: u32) -> Self {
        let mut self_ = Self { affinity: -1, data: Vec::with_capacity(count) };

        for _ in 0..count {
            self_.add(CpuThread::new(-1, Some(intensity)));
        }

        self_
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn add(&mut self, thread: CpuThread) {
        self.data.push(thread)
    }

    pub fn threads(&self) -> &[CpuThread] {
        &self.data
    }
}

#[inline]
pub fn get_affinity(index: u64, affinity: i32) -> i32 {
    if affinity < 0 {
        return -1
    }

    let affinity = affinity as u64;
    let mut idx = 0u64;

    for i in 0..64 {
        if (affinity & (1u64 << i)) == 0 {
            continue
        }

        if idx == index {
            return i
        }

        idx += 1;
    }

    -1
}
