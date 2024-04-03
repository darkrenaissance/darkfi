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

// https://stackoverflow.com/questions/35901547/how-can-i-find-a-subsequence-in-a-u8-slice
pub fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// Extra methods for Iterator. Copied from [itertools](https://github.com/rust-itertools/itertools).
///
/// Licensed under MIT.
pub trait Itertools: Iterator {
    /// `.try_collect()` is more convenient way of writing
    /// `.collect::<Result<_, _>>()`
    ///
    /// # Example
    ///
    /// ```
    /// use std::{fs, io};
    /// use itertools::Itertools;
    ///
    /// fn process_dir_entries(entries: &[fs::DirEntry]) {
    ///     // ...
    /// }
    ///
    /// fn do_stuff() -> std::io::Result<()> {
    ///     let entries: Vec<_> = fs::read_dir(".")?.try_collect()?;
    ///     process_dir_entries(&entries);
    ///
    ///     Ok(())
    /// }
    /// ```
    fn try_collect<T, U, E>(self) -> Result<U, E>
    where
        Self: Sized + Iterator<Item = Result<T, E>>,
        Result<U, E>: FromIterator<Result<T, E>>,
    {
        self.collect()
    }
}

impl<T> Itertools for T where T: Iterator + ?Sized {}

pub trait NextTuple3<I>: Iterator<Item = I> {
    fn next_tuple(&mut self) -> Option<(I, I, I)>;
}

impl<I: Iterator<Item = T>, T> NextTuple3<T> for I {
    fn next_tuple(&mut self) -> Option<(T, T, T)> {
        let a = self.next()?;
        let b = self.next()?;
        let c = self.next()?;
        Some((a, b, c))
    }
}

pub trait NextTuple4<I>: Iterator<Item = I> {
    fn next_tuple(&mut self) -> Option<(I, I, I, I)>;
}

impl<I: Iterator<Item = T>, T> NextTuple4<T> for I {
    fn next_tuple(&mut self) -> Option<(T, T, T, T)> {
        let a = self.next()?;
        let b = self.next()?;
        let c = self.next()?;
        let d = self.next()?;
        Some((a, b, c, d))
    }
}
