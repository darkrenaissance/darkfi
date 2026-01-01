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

use halo2_proofs::arithmetic::Field;
use pasta_curves::Fp;
use rand::rngs::OsRng;

use super::*;

#[test]
fn check_empties() {
    use empty::EMPTY_NODES_FP;
    let hasher = Poseidon::<Fp, 2>::new();
    let empty_leaf = Fp::from(0);
    let empty_nodes = gen_empty_nodes::<{ SMT_FP_DEPTH + 1 }, _, _>(&hasher, empty_leaf);
    assert_eq!(empty_nodes.len(), SMT_FP_DEPTH + 1);
    for (left, right) in empty_nodes.iter().zip(EMPTY_NODES_FP.iter()) {
        //println!("  Fp::from_raw([{:?}]),", left);
        assert_eq!(left, right);
    }
}

#[test]
fn empties() {
    let hasher = Poseidon::<Fp, 2>::new();
    let empty_leaf = Fp::from(0);
    let empty_nodes = gen_empty_nodes::<{ 3 + 1 }, _, _>(&hasher, empty_leaf);

    let empty_node1 = hasher.hash([empty_leaf, empty_leaf]);
    let empty_node2 = hasher.hash([empty_node1, empty_node1]);
    let empty_root = hasher.hash([empty_node2, empty_node2]);

    assert_eq!(empty_nodes[3], empty_leaf);
    assert_eq!(empty_nodes[2], empty_node1);
    assert_eq!(empty_nodes[1], empty_node2);
    assert_eq!(empty_nodes[0], empty_root);
}

#[test]
fn poseidon_smt() {
    const HEIGHT: usize = 3;
    let hasher = Poseidon::<Fp, 2>::new();
    let empty_leaf = Fp::from(0);
    let empty_nodes = gen_empty_nodes::<{ HEIGHT + 1 }, _, _>(&hasher, empty_leaf);

    let store = MemoryStorage::<Fp>::new();
    let mut smt = SparseMerkleTree::<HEIGHT, { HEIGHT + 1 }, _, _, _>::new(
        store,
        hasher.clone(),
        &empty_nodes,
    );

    // Both reprs should match
    assert_eq!(Fp::from(1).as_biguint(), BigUint::from(1u32));
    assert_eq!(Fp::from(300).as_biguint(), BigUint::from(300u32));

    let leaves = vec![
        (Fp::from(1), Fp::random(&mut OsRng)),
        (Fp::from(2), Fp::random(&mut OsRng)),
        (Fp::from(3), Fp::random(&mut OsRng)),
    ];
    smt.insert_batch(leaves.clone()).unwrap();

    let hash1 = leaves[0].1;
    let hash2 = leaves[1].1;
    let hash3 = leaves[2].1;

    let hash = |l, r| hasher.hash([l, r]);

    let hash01 = hash(empty_nodes[3], hash1);
    let hash23 = hash(hash2, hash3);

    let hash0123 = hash(hash01, hash23);
    let root = hash(hash0123, empty_nodes[1]);
    assert_eq!(root, smt.root());

    //println!("hash1: {:?}", hash1);
    //println!("hash2: {:?}", hash2);
    //println!("hash3: {:?}", hash3);
    //println!("hash4-7: {:?}", empty_nodes[3]);
    //println!();
    //println!("hash01: {:?}", hash01);
    //println!("hash23: {:?}", hash23);
    //println!("hash45: {:?}", empty_nodes[2]);
    //println!("hash67: {:?}", empty_nodes[2]);
    //println!();
    //println!("hash0123: {:?}", hash0123);
    //println!("hash4567: {:?}", empty_nodes[1]);
    //println!();
    //println!("root: {:?}", root);
    //println!();

    // Now try to construct a membership proof for leaf 3
    let pos = leaves[2].0;
    let path = smt.prove_membership(&pos);
    assert_eq!(path.path[0], empty_nodes[1]);
    assert_eq!(path.path[1], hash01);
    assert_eq!(path.path[2], hash2);

    assert_eq!(hash23, hash(path.path[2], hash3));
    assert_eq!(hash0123, hash(path.path[1], hash(path.path[2], hash3)));
    assert_eq!(root, hash(hash(path.path[1], hash(path.path[2], hash3)), path.path[0]));

    //println!("path0: {:?}", path.path[0]);
    //println!("path1: {:?}", path.path[1]);
    //println!("path2: {:?}", path.path[2]);

    assert!(path.verify(&root, &hash3, &pos));
}

#[test]
fn poseidon_smt_incl_proof() {
    const HEIGHT: usize = 3;
    let hasher = Poseidon::<Fp, 2>::new();
    let empty_leaf = Fp::from(0);
    let empty_nodes = gen_empty_nodes::<{ HEIGHT + 1 }, _, _>(&hasher, empty_leaf);

    let store = MemoryStorage::<Fp>::new();
    let mut smt = SparseMerkleTree::<HEIGHT, { HEIGHT + 1 }, _, _, _>::new(
        store,
        hasher.clone(),
        &empty_nodes,
    );

    let leaves = vec![
        (Fp::from(1), Fp::random(&mut OsRng)),
        (Fp::from(2), Fp::random(&mut OsRng)),
        (Fp::from(3), Fp::random(&mut OsRng)),
        /*
        (Fp::from(1), Fp::from(111)),
        (Fp::from(2), Fp::from(222)),
        (Fp::from(3), Fp::from(333)),
        */
    ];
    smt.insert_batch(leaves.clone()).unwrap();

    let (pos, leaf) = leaves[2];
    assert_eq!(smt.get_leaf(&pos), leaf);

    let path = smt.prove_membership(&pos);
    assert!(path.verify(&smt.root(), &leaf, &pos));

    smt.remove_leaves(vec![(pos, leaf)]).unwrap();
    assert!(!path.verify(&smt.root(), &leaf, &pos));
}
