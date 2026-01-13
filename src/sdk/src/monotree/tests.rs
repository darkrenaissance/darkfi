/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 * Copyright (C) 2021 MONOLOG (Taeho Francis Lim and Jongwhan Lee) MIT License
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

use super::{
    tree::{verify_proof, MemoryDb},
    utils::{random_hashes, shuffle},
    Hash, Monotree,
};

#[test]
fn monotree_test_insert_then_verify_values() {
    let keys = random_hashes(100);
    let values = random_hashes(100);

    let mut root = None;
    let db = MemoryDb::new();
    let mut tree = Monotree::new(db);

    for (i, (key, value)) in keys.iter().zip(values.iter()).enumerate() {
        root = tree.insert(root.as_ref(), key, value).unwrap();
        tree.set_headroot(root.as_ref());

        for (k, v) in keys.iter().zip(values.iter()).take(i + 1) {
            assert_eq!(tree.get(root.as_ref(), k).unwrap(), Some(*v));
        }
    }

    assert_ne!(root, None);
}

#[test]
fn monotree_test_insert_keys_then_gen_and_verify_proof() {
    let keys = random_hashes(100);
    let values = random_hashes(100);

    let mut root = None;
    let db = MemoryDb::new();
    let mut tree = Monotree::new(db);

    for (i, (key, value)) in keys.iter().zip(values.iter()).enumerate() {
        root = tree.insert(root.as_ref(), key, value).unwrap();
        tree.set_headroot(root.as_ref());

        for (k, v) in keys.iter().zip(values.iter()).take(i + 1) {
            let proof = tree.get_merkle_proof(root.as_ref(), k).unwrap();
            assert!(verify_proof(root.as_ref(), v, proof.as_ref()));
        }
    }

    assert_ne!(root, None);
}

#[test]
fn monotree_test_insert_keys_then_delete_keys_in_order() {
    let keys = random_hashes(100);
    let values = random_hashes(100);

    let mut root = None;
    let db = MemoryDb::new();
    let mut tree = Monotree::new(db);

    // pre-insertion for removal test
    root = tree.inserts(root.as_ref(), &keys, &values).unwrap();
    tree.set_headroot(root.as_ref());

    // Removal test with keys in order
    for (i, (key, _)) in keys.iter().zip(values.iter()).enumerate() {
        assert_ne!(root, None);
        // Assert that all other values are fine after deletion
        for (k, v) in keys.iter().zip(values.iter()).skip(i) {
            assert_eq!(tree.get(root.as_ref(), k).unwrap(), Some(*v));
            let proof = tree.get_merkle_proof(root.as_ref(), k).unwrap();
            assert!(verify_proof(root.as_ref(), v, proof.as_ref()));
        }

        // Delete a key and check if it worked
        root = tree.remove(root.as_ref(), key).unwrap();
        tree.set_headroot(root.as_ref());
        assert_eq!(tree.get(root.as_ref(), key).unwrap(), None);
    }

    // Back to initial state of tree
    assert_eq!(root, None);
}

#[test]
fn monotree_test_insert_then_delete_keys_reverse() {
    let keys = random_hashes(100);
    let values = random_hashes(100);

    let mut root = None;
    let db = MemoryDb::new();
    let mut tree = Monotree::new(db);

    // pre-insertion for removal test
    root = tree.inserts(root.as_ref(), &keys, &values).unwrap();
    tree.set_headroot(root.as_ref());

    // Removal test with keys in reverse order
    for (i, (key, _)) in keys.iter().zip(values.iter()).rev().enumerate() {
        assert_ne!(root, None);
        // Assert that all other values are fine after deletion
        for (k, v) in keys.iter().zip(values.iter()).rev().skip(i) {
            assert_eq!(tree.get(root.as_ref(), k).unwrap(), Some(*v));
            let proof = tree.get_merkle_proof(root.as_ref(), k).unwrap();
            assert!(verify_proof(root.as_ref(), v, proof.as_ref()));
        }

        // Delete a key and check if it worked
        root = tree.remove(root.as_ref(), key).unwrap();
        tree.set_headroot(root.as_ref());
        assert_eq!(tree.get(root.as_ref(), key).unwrap(), None);
    }

    // Back to initial state of tree
    assert_eq!(root, None);
}

#[test]
fn monotree_test_insert_then_delete_keys_random() {
    let keys = random_hashes(100);
    let values = random_hashes(100);

    let mut root = None;
    let db = MemoryDb::new();
    let mut tree = Monotree::new(db);

    // pre-insertion for removal test
    root = tree.inserts(root.as_ref(), &keys, &values).unwrap();
    tree.set_headroot(root.as_ref());

    // Shuffles keys/leaves' index for imitating random access
    let mut idx: Vec<usize> = (0..keys.len()).collect();
    shuffle(&mut idx);

    // Test with shuffled keys
    for (n, i) in idx.iter().enumerate() {
        assert_ne!(root, None);

        // Assert that all values are fine after deletion
        for j in idx.iter().skip(n) {
            assert_eq!(tree.get(root.as_ref(), &keys[*j]).unwrap(), Some(values[*j]));
            let proof = tree.get_merkle_proof(root.as_ref(), &keys[*j]).unwrap();
            assert!(verify_proof(root.as_ref(), &values[*j], proof.as_ref()));
        }

        // Delete a key by random index and check if it worked
        root = tree.remove(root.as_ref(), &keys[*i]).unwrap();
        tree.set_headroot(root.as_ref());
        assert_eq!(tree.get(root.as_ref(), &values[*i]).unwrap(), None);
    }

    // Back to initial state of tree
    assert_eq!(root, None);
}

#[test]
fn monotree_test_deterministic_ordering() {
    let keys = random_hashes(100);
    let values = random_hashes(100);

    let mut root1 = None;
    let db = MemoryDb::new();
    let mut tree1 = Monotree::new(db);

    let mut root2 = None;
    let db = MemoryDb::new();
    let mut tree2 = Monotree::new(db);

    // Insert in normal order
    root1 = tree1.inserts(root1.as_ref(), &keys, &values).unwrap();
    tree1.set_headroot(root1.as_ref());
    assert_ne!(root1, None);

    // Insert in reverse order
    let rev_keys: Vec<Hash> = keys.iter().rev().cloned().collect();
    let rev_vals: Vec<Hash> = values.iter().rev().cloned().collect();
    root2 = tree2.inserts(root2.as_ref(), &rev_keys, &rev_vals).unwrap();
    tree2.set_headroot(root2.as_ref());
    assert_ne!(root2, None);

    // Verify roots match
    assert_eq!(root1, root2);

    // Verify removal consistency
    for key in keys {
        root1 = tree1.remove(root1.as_ref(), &key).unwrap();
        tree1.set_headroot(root1.as_ref());

        root2 = tree2.remove(root2.as_ref(), &key).unwrap();
        tree2.set_headroot(root2.as_ref());

        assert_eq!(root1, root2);
    }

    assert_eq!(root1, None);
    assert_eq!(root2, None);
}

#[test]
fn monotree_test_insert_remove_identity() {
    let keys = random_hashes(3);
    let values = random_hashes(3);

    println!("{:?}", keys);
    println!("{:?}", values);

    let db = MemoryDb::new();
    let mut tree = Monotree::new(db);
    let mut root = tree.inserts(None, &keys, &values).unwrap();
    let original_root = root;

    // Insert then remove a new key
    let temp_key = random_hashes(1)[0];
    let temp_value = random_hashes(1)[0];
    root = tree.insert(root.as_ref(), &temp_key, &temp_value).unwrap();
    root = tree.remove(root.as_ref(), &temp_key).unwrap();

    assert_eq!(root, original_root);
}
