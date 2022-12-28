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

use crate::{patch::OpMethod, util::str_to_chars};

pub struct Lcs<'a> {
    a: Vec<&'a str>,
    b: Vec<&'a str>,
    lengths: Vec<Vec<u64>>,
}

impl<'a> Lcs<'a> {
    pub fn new(a: &'a str, b: &'a str) -> Self {
        let a: Vec<_> = str_to_chars(a);
        let b: Vec<_> = str_to_chars(b);
        let (na, nb) = (a.len(), b.len());

        let mut lengths = vec![vec![0; nb + 1]; na + 1];

        for (i, ci) in a.iter().enumerate() {
            for (j, cj) in b.iter().enumerate() {
                lengths[i + 1][j + 1] = if ci == cj {
                    lengths[i][j] + 1
                } else {
                    lengths[i][j + 1].max(lengths[i + 1][j])
                }
            }
        }

        Self { a, b, lengths }
    }

    fn op(&self, ops: &mut Vec<OpMethod>, i: usize, j: usize) {
        if i == 0 && j == 0 {
            return
        }

        if i == 0 {
            ops.push(OpMethod::Insert(self.b[j - 1].to_string()));
            self.op(ops, i, j - 1);
        } else if j == 0 {
            ops.push(OpMethod::Delete((1) as _));
            self.op(ops, i - 1, j);
        } else if self.a[i - 1] == self.b[j - 1] {
            ops.push(OpMethod::Retain((1) as _));
            self.op(ops, i - 1, j - 1);
        } else if self.lengths[i - 1][j] > self.lengths[i][j - 1] {
            ops.push(OpMethod::Delete((1) as _));
            self.op(ops, i - 1, j);
        } else {
            ops.push(OpMethod::Insert(self.b[j - 1].to_string()));
            self.op(ops, i, j - 1);
        }
    }

    pub fn ops(&self) -> Vec<OpMethod> {
        let mut ops = vec![];
        self.op(&mut ops, self.a.len(), self.b.len());
        ops.reverse();
        ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lcs() {
        let lcs = Lcs::new("hello", "test hello");
        assert_eq!(
            lcs.ops(),
            vec![
                OpMethod::Insert("t".into()),
                OpMethod::Insert("e".into()),
                OpMethod::Insert("s".into()),
                OpMethod::Insert("t".into()),
                OpMethod::Insert(" ".into()),
                OpMethod::Retain(1),
                OpMethod::Retain(1),
                OpMethod::Retain(1),
                OpMethod::Retain(1),
                OpMethod::Retain(1),
            ]
        );

        let lcs = Lcs::new("hello world", "hello");
        assert_eq!(
            lcs.ops(),
            vec![
                OpMethod::Retain(1),
                OpMethod::Retain(1),
                OpMethod::Retain(1),
                OpMethod::Retain(1),
                OpMethod::Delete(1),
                OpMethod::Delete(1),
                OpMethod::Delete(1),
                OpMethod::Retain(1),
                OpMethod::Delete(1),
                OpMethod::Delete(1),
                OpMethod::Delete(1),
            ]
        );
    }
}
