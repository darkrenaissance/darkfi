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

#[derive(Clone)]
pub struct RingBuffer<T, const N: usize> {
    vals: [Option<T>; N],
    head: i64,
    tail: i64,
}

impl<T, const N: usize> RingBuffer<T, N> {
    const LEN: usize = N;

    pub fn new() -> Self {
        Self { vals: [const { None }; N], head: -1, tail: -1 }
    }

    pub fn push(&mut self, v: T) {
        let len = Self::LEN as i64;

        self.head = (self.head + 1) % len;
        if self.head == self.tail {
            self.tail = (self.tail + 1) % len;
        }

        if self.tail < 0 {
            self.tail = 0;
        }

        let _ = std::mem::replace(&mut self.vals[self.head as usize], Some(v));
    }

    pub fn head(&self) -> Option<&T> {
        if self.head < 0 {
            return None
        }
        Some(self.vals[self.head as usize].as_ref().unwrap())
    }

    pub fn tail(&self) -> Option<&T> {
        if self.tail < 0 {
            return None
        }
        Some(self.vals[self.tail as usize].as_ref().unwrap())
    }
}
