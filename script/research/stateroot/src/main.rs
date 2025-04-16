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

use std::{collections::BTreeMap, time::Instant};

use anyhow::Result;
use blake3::KEY_LEN;
use rand::Rng;

const EMPTY_HASH: [u8; KEY_LEN] = [0xFF; KEY_LEN];

#[derive(Debug)]
struct SparseMerkleTree {
    root: [u8; KEY_LEN],
    base_level: BTreeMap<[u8; KEY_LEN], [u8; KEY_LEN]>,
    cache: BTreeMap<usize, BTreeMap<[u8; KEY_LEN], [u8; KEY_LEN]>>,
}

impl SparseMerkleTree {
    fn new() -> Self {
        Self { root: EMPTY_HASH, base_level: BTreeMap::new(), cache: BTreeMap::new() }
    }

    fn hash_nodes(key: &[u8; KEY_LEN], value: &[u8; KEY_LEN]) -> [u8; KEY_LEN] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(key);
        hasher.update(value);
        *hasher.finalize().as_bytes()
    }

    fn recalculate_root(&mut self) {
        // Level 0
        let mut hashes: Vec<[u8; KEY_LEN]> = self
            .base_level
            .iter()
            .map(|(key, value)| {
                *self
                    .cache
                    .entry(0)
                    .or_insert_with(BTreeMap::new)
                    .entry(*key)
                    .or_insert_with(|| Self::hash_nodes(key, value))
            })
            .collect();

        let mut levels = vec![];

        // Iteratively combine hashes until we get to the root
        let mut level_index = 1;
        while hashes.len() > 1 {
            let mut parent_hashes = vec![];
            let mut current_level = vec![];

            for chunk in hashes.chunks(2) {
                let left = chunk[0];
                let right = if chunk.len() == 2 { chunk[1] } else { EMPTY_HASH };
                let parent_hash = Self::hash_nodes(&left, &right);
                parent_hashes.push(parent_hash);

                current_level.push((left, right));
            }

            // Store the current level in the cache
            self.cache.insert(
                level_index,
                current_level
                    .clone()
                    .into_iter()
                    .map(|(left, right)| {
                        (Self::hash_nodes(&left, &right), Self::hash_nodes(&left, &right))
                    })
                    .collect(),
            );

            levels.push(current_level);

            // Move to the next level of the tree
            hashes = parent_hashes;
            level_index += 1;
        }

        // Final hash is the root
        self.root = hashes[0];
        self.visualise(levels);
    }

    fn visualise(&self, levels: Vec<Vec<([u8; KEY_LEN], [u8; KEY_LEN])>>) {
        for (level_index, level) in levels.iter().enumerate() {
            println!("Level {}:", level_index);
            for (left, right) in level {
                println!(
                    "  Left: {}  Right: {} => Parent: {}",
                    blake3::Hash::from(*left),
                    blake3::Hash::from(*right),
                    blake3::Hash::from(Self::hash_nodes(left, right))
                );
            }
        }

        println!("Root: {}", blake3::Hash::from(self.root));
    }

    fn insert(&mut self, key: [u8; KEY_LEN], value: [u8; KEY_LEN]) {
        self.base_level.insert(key, value);
        self.recalculate_root();
    }
}

fn sled_tree_csum(tree: &sled::Tree) -> Result<[u8; KEY_LEN]> {
    let mut hasher = blake3::Hasher::new();

    for elem in tree.iter() {
        let elem = elem?;
        hasher.update(&elem.0);
        hasher.update(&elem.1);
    }

    Ok(*hasher.finalize().as_bytes())
}

fn main() -> Result<()> {
    let sled_db = sled::Config::new().temporary(true).open()?;
    let mut state_tree = SparseMerkleTree::new();
    let mut rng = rand::thread_rng();

    let mut trees = vec![];

    for i in 0..1000 {
        let tree_name = format!("tree_{}", i);
        let tree_name_hash = blake3::hash(&tree_name.as_bytes());
        let tree = sled_db.open_tree(&tree_name)?;

        trees.push((*tree_name_hash.as_bytes(), tree.clone()));

        let n_entries = rng.gen_range(1..10001);

        for _ in 0..n_entries {
            let key = format!("key_{}", rng.gen::<u64>());
            let value = format!("value_{}", rng.gen::<u64>());

            tree.insert(key.as_bytes(), value.as_bytes())?;
        }
    }

    let now = Instant::now();
    for (name, tree) in trees {
        let csum = sled_tree_csum(&tree)?;
        state_tree.insert(name, csum);
    }
    println!("{:?}", now.elapsed());

    Ok(())
}
