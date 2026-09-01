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

//! A Fenwick tree (binary indexed tree) over `f32` values.
//!
//! Answers prefix-sum questions over mutable heights in O(log n):
//! total sums, the item containing a given cumulative offset
//! (`lower_bound`), and point updates. The internal layout is the
//! classic implicit one: a flat 1-indexed `Vec<f32>` where node `i`
//! holds the partial sum of the `i & -i` items ending at `i`.

/// Fenwick tree over non-negative `f32` summands.
///
/// All public indices are 0-based item positions.
#[derive(Debug)]
pub struct Fenwick {
    /// Partial sums, 1-indexed; node `i` covers the `(i & -i)` items ending at `i`.
    /// Slot 0 is unused padding.
    tree: Vec<f32>,
    /// Number of items.
    len: usize,
}

impl Fenwick {
    /// Build from item values in display order (O(n)).
    pub fn new(vals: &[f32]) -> Self {
        let mut fenwick = Self { tree: Vec::with_capacity(vals.len() + 1), len: vals.len() };
        fenwick.build(vals);
        fenwick
    }

    /// The empty tree.
    pub fn empty() -> Self {
        Self { tree: vec![0.], len: 0 }
    }

    /// Append an item (O(log n)) — the live-arrival hot path.
    pub fn push(&mut self, val: f32) {
        let i = self.len + 1;
        self.tree.push(val);
        let lowbit = i & i.wrapping_neg();
        // The new node must hold the sum of the lowbit items ending at
        // i: the appended value plus the items already covered by the
        // nodes directly below it.
        let below = self.prefix(i - 1) - self.prefix(i - lowbit);
        self.tree[i] = val + below;
        self.len = i;
    }

    /// Current value at item `idx` (O(log n)).
    ///
    /// ## Panics
    ///
    /// If `idx` is out of bounds.
    pub fn get(&self, idx: usize) -> f32 {
        self.assert_index(idx);
        self.prefix(idx + 1) - self.prefix(idx)
    }

    /// Add `delta` to the item at `idx` (O(log n)) — height changes.
    ///
    /// ## Panics
    ///
    /// If `idx` is out of bounds.
    pub fn add(&mut self, idx: usize, delta: f32) {
        self.assert_index(idx);
        let mut i = idx + 1;
        while i <= self.len {
            self.tree[i] += delta;
            i += i & i.wrapping_neg();
        }
    }

    /// Overwrite the item at `idx` with `val` (O(log n)).
    ///
    /// ## Panics
    ///
    /// If `idx` is out of bounds.
    pub fn set(&mut self, idx: usize, val: f32) {
        let delta = val - self.get(idx);
        self.add(idx, delta);
    }

    /// Sum of items `[0, idx)` (O(log n)) — `total_height`, `pos_of`.
    pub fn prefix(&self, idx: usize) -> f32 {
        let mut i = idx.min(self.len);
        let mut sum = 0.;
        while i > 0 {
            sum += self.tree[i];
            i -= i & i.wrapping_neg();
        }
        sum
    }

    /// Sum of items `[from, to)` (O(log n)).
    pub fn range(&self, from: usize, to: usize) -> f32 {
        let from = from.min(self.len);
        let to = to.min(self.len);
        if from >= to {
            return 0.
        }
        self.prefix(to) - self.prefix(from)
    }

    /// The first item index whose cumulative sum (items `0..=idx`)
    /// exceeds `target` (O(log n)) — px-from-bottom position → item
    /// lookup. Returns `len` when `target` is at or past the total sum.
    pub fn lower_bound(&self, target: f32) -> usize {
        if self.len == 0 || target < 0. {
            return 0
        }

        let mut pw = 1usize << self.len.ilog2();
        let mut pos = 0usize;
        let mut rem = target;
        while pw != 0 {
            let next = pos + pw;
            if next <= self.len && self.tree[next] <= rem {
                pos = next;
                rem -= self.tree[next];
            }
            pw >>= 1;
        }

        // pos is the largest 1-based index with prefix(pos) <= target,
        // so item `pos` (0-based) is the first whose cumulative sum
        // exceeds the target.
        pos
    }

    /// The first item index whose cumulative sum of the items BEFORE it
    /// (`prefix(idx)`) is at least `target` (O(log n)) — the exclusive
    /// end of the item window intersecting a given cumulative offset.
    /// Returns `len` when `target` is at or past the total sum.
    pub fn lower_bound_prefix(&self, target: f32) -> usize {
        if self.len == 0 || target <= 0. {
            return 0
        }

        let mut pw = 1usize << self.len.ilog2();
        let mut pos = 0usize;
        let mut rem = target;
        while pw != 0 {
            let next = pos + pw;
            if next <= self.len && self.tree[next] < rem {
                pos = next;
                rem -= self.tree[next];
            }
            pw >>= 1;
        }

        // pos is the largest index with prefix(pos) < target; the first
        // index at-or-after the target follows. Clamped to len when the
        // target exceeds the total sum (no such index exists).
        let res = pos + 1;
        if res > self.len {
            self.len
        } else {
            res
        }
    }

    /// Full rebuild from new item values (O(n)) — structural batches.
    pub fn rebuild(&mut self, vals: &[f32]) {
        self.len = vals.len();
        self.build(vals);
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the tree holds no items.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// O(n) in-place construction from item values.
    fn build(&mut self, vals: &[f32]) {
        self.tree.clear();
        self.tree.resize(vals.len() + 1, 0.);
        for (i, val) in vals.iter().enumerate() {
            let i = i + 1;
            self.tree[i] += val;
            let parent = i + (i & i.wrapping_neg());
            if parent <= vals.len() {
                self.tree[parent] += self.tree[i];
            }
        }
    }

    fn assert_index(&self, idx: usize) {
        assert!(idx < self.len, "fenwick index {idx} out of bounds (len {})", self.len);
    }
}

impl Default for Fenwick {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, Rng, SeedableRng};

    /// Brute-force reference: the first idx whose cumulative sum exceeds target.
    fn brute_lower_bound(vals: &[f32], target: f32) -> usize {
        let mut sum = 0.;
        for (idx, val) in vals.iter().enumerate() {
            sum += val;
            if sum > target {
                return idx
            }
        }
        vals.len()
    }

    /// Brute-force reference: the first idx whose before-it sum
    /// (prefix) is at least the target.
    fn brute_lower_bound_prefix(vals: &[f32], target: f32) -> usize {
        let mut sum = 0.;
        for idx in 0..=vals.len() {
            if sum >= target {
                return idx
            }
            if idx < vals.len() {
                sum += vals[idx];
            }
        }
        vals.len()
    }

    /// Random item values are integer-valued floats: their sums are
    /// exact in f32 at these magnitudes, so tree accumulations in any
    /// order match brute force exactly and lower_bound boundaries are
    /// stable. (Fractional drift is a separate, accepted concern noted
    /// in the design; it is not observable with exact arithmetic.)
    fn gen_val(rng: &mut StdRng) -> f32 {
        rng.gen_range(0..1000) as f32
    }

    /// Check every Fenwick query against the brute-force reference.
    fn check_all(fenwick: &Fenwick, vals: &[f32]) {
        assert_eq!(fenwick.len(), vals.len());
        assert_eq!(fenwick.is_empty(), vals.is_empty());

        let mut brute = 0.;
        for i in 0..=vals.len() {
            assert!((fenwick.prefix(i) - brute).abs() < 1e-3, "prefix({i})");
            brute += vals.get(i).copied().unwrap_or(0.);
        }

        for i in 0..vals.len() {
            assert!((fenwick.get(i) - vals[i]).abs() < 1e-3, "get({i})");
        }

        for from in 0..=vals.len() {
            for to in from..=vals.len() {
                let mut brute = 0.;
                for val in &vals[from..to] {
                    brute += val;
                }
                assert!((fenwick.range(from, to) - brute).abs() < 1e-3, "range({from},{to})");
            }
        }

        let mut total = 0.;
        for val in vals {
            total += val;
        }
        let mut targets = vec![0., total, total - 0.5, total + 1., -1.];
        let mut acc = 0.;
        for val in vals {
            targets.push(acc);
            targets.push(acc + val / 2.);
            targets.push(acc + val);
            acc += val;
        }
        for target in targets {
            assert_eq!(
                fenwick.lower_bound(target),
                brute_lower_bound(vals, target),
                "lower_bound({target})"
            );
            assert_eq!(
                fenwick.lower_bound_prefix(target),
                brute_lower_bound_prefix(vals, target),
                "lower_bound_prefix({target})"
            );
        }
    }

    #[test]
    fn empty_tree() {
        let fenwick = Fenwick::new(&[]);
        assert!(fenwick.is_empty());
        assert_eq!(fenwick.prefix(0), 0.);
        assert_eq!(fenwick.prefix(10), 0.);
        assert_eq!(fenwick.lower_bound(0.), 0);
        assert_eq!(fenwick.range(0, 0), 0.);
    }

    #[test]
    fn single_item() {
        let mut fenwick = Fenwick::new(&[42.]);
        assert_eq!(fenwick.len(), 1);
        assert_eq!(fenwick.prefix(0), 0.);
        assert_eq!(fenwick.prefix(1), 42.);
        assert_eq!(fenwick.get(0), 42.);
        assert_eq!(fenwick.lower_bound(0.), 0);
        assert_eq!(fenwick.lower_bound(41.9), 0);
        assert_eq!(fenwick.lower_bound(42.), 1);
        fenwick.set(0, 7.);
        assert_eq!(fenwick.prefix(1), 7.);
        check_all(&fenwick, &[7.]);
    }

    #[test]
    fn zero_height_items() {
        let vals = [0., 0., 5., 0., 3.];
        let fenwick = Fenwick::new(&vals);
        check_all(&fenwick, &vals);
        assert_eq!(fenwick.lower_bound(0.), 2);
        assert_eq!(fenwick.lower_bound(4.9), 2);
        assert_eq!(fenwick.lower_bound(5.), 4);
    }

    #[test]
    fn push_matches_build() {
        let mut rng = StdRng::seed_from_u64(0xC0FFEE);
        for _ in 0..20 {
            let n = rng.gen_range(1..200);
            let mut vals = vec![];
            for _ in 0..n {
                vals.push(gen_val(&mut rng));
            }
            let built = Fenwick::new(&vals);
            let mut pushed = Fenwick::empty();
            for val in &vals {
                pushed.push(*val);
            }
            assert_eq!(built.len(), pushed.len());
            for i in 0..=n {
                assert!((built.prefix(i) - pushed.prefix(i)).abs() < 1e-3, "prefix({i})");
            }
            check_all(&pushed, &vals);
        }
    }

    #[test]
    fn randomized_operations() {
        let mut rng = StdRng::seed_from_u64(0xBADF00D);
        for _ in 0..30 {
            let n = rng.gen_range(0..300);
            let mut vals = vec![];
            for _ in 0..n {
                vals.push(gen_val(&mut rng));
            }
            let mut fenwick = Fenwick::new(&vals);
            check_all(&fenwick, &vals);

            for _ in 0..500 {
                match rng.gen_range(0..5) {
                    0 => {
                        let val = gen_val(&mut rng);
                        vals.push(val);
                        fenwick.push(val);
                    }
                    1 => {
                        if vals.is_empty() {
                            continue
                        }
                        let idx = rng.gen_range(0..vals.len());
                        let delta = rng.gen_range(-500..500) as f32;
                        // Heights stay non-negative, so clamp shrinking deltas.
                        let applied = delta.max(-vals[idx]);
                        vals[idx] += applied;
                        fenwick.add(idx, applied);
                    }
                    2 => {
                        if vals.is_empty() {
                            continue
                        }
                        let idx = rng.gen_range(0..vals.len());
                        let val = gen_val(&mut rng);
                        vals[idx] = val;
                        fenwick.set(idx, val);
                    }
                    3 => {
                        let n = rng.gen_range(0..100);
                        vals.clear();
                        for _ in 0..n {
                            vals.push(gen_val(&mut rng));
                        }
                        fenwick.rebuild(&vals);
                    }
                    _ => {
                        check_all(&fenwick, &vals);
                    }
                }
            }

            check_all(&fenwick, &vals);
        }
    }

    #[test]
    fn rebuild_replaces_everything() {
        let mut fenwick = Fenwick::new(&[100., 200., 300.]);
        fenwick.rebuild(&[1., 2.]);
        assert_eq!(fenwick.len(), 2);
        check_all(&fenwick, &[1., 2.]);
        fenwick.rebuild(&[]);
        assert!(fenwick.is_empty());
        check_all(&fenwick, &[]);
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn get_out_of_bounds_panics() {
        let fenwick = Fenwick::new(&[1.]);
        fenwick.get(1);
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn add_out_of_bounds_panics() {
        let mut fenwick = Fenwick::new(&[1.]);
        fenwick.add(5, 1.);
    }
}
