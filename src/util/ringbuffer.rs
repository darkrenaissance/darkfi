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

use std::collections::{vec_deque::Iter, VecDeque};

/// A ring buffer of fixed capacity
#[derive(Default, Eq, PartialEq, Clone, Debug)]
pub struct RingBuffer<T, const N: usize>(VecDeque<T>);

impl<T: Eq + PartialEq + Clone, const N: usize> RingBuffer<T, N> {
    /// Create a new [`RingBuffer`] with given fixed capacity
    pub fn new() -> RingBuffer<T, N> {
        Self(VecDeque::with_capacity(N))
    }

    /// Push an element to the back of the `RingBuffer`, removing
    /// the front element in case the buffer is full.
    pub fn push(&mut self, value: T) {
        if self.0.len() == N {
            self.0.pop_front();
        }
        self.0.push_back(value);
    }

    /// Returns the current number of items in the buffer
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if buffer is empty, false otherwise
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Removes and returns the oldest item in the buffer
    pub fn pop(&mut self) -> Option<T> {
        self.0.pop_front()
    }

    /// Returns a front-to-back iterator
    pub fn iter(&self) -> Iter<'_, T> {
        self.0.iter()
    }

    /// Returns true if the buffer contains an element equal to the given value
    pub fn contains(&self, x: &T) -> bool {
        self.0.contains(x)
    }

    /// Provides a reference to the back element, or `None` if empty.
    pub fn back(&self) -> Option<&T> {
        self.0.back()
    }

    /// Cast the ringbuffer into a vec
    pub fn to_vec(&self) -> Vec<T> {
        self.0.iter().cloned().collect()
    }

    /// Rearranges the internal storage of this deque so it is one contiguous slice.
    pub fn make_contiguous(&mut self) -> &mut [T] {
        self.0.make_contiguous()
    }
}

impl<T, const N: usize> std::ops::Index<usize> for RingBuffer<T, N> {
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &T {
        self.0.get(index).expect("Out of bounds access")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behaviour() {
        const BUF_SIZE: usize = 10;
        let mut buf = RingBuffer::<usize, BUF_SIZE>::new();

        for i in 0..BUF_SIZE {
            buf.push(i);
        }

        assert!(!buf.is_empty());
        assert!(buf.len() == BUF_SIZE);

        for i in 0..BUF_SIZE {
            buf.push(i + 10);
        }

        assert!(buf.len() == BUF_SIZE);

        for (i, v) in buf.iter().enumerate() {
            assert_eq!(*v, i + 10);
        }
    }
}
