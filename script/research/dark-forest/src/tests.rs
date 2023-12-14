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

use crate::{dark_leaf_vec_integrity_check, DarkLeaf, DarkTree, DarkTreeResult};

/// Gereate a predefined [`DarkTree`] along with its
/// expected traversal order.
///
/// Tree structure:
///                        22
///           /             |            \
///          10            14            21
///      /  /  \   \      /  \      /     |   \
///     2   4   6   9    12  13    17    18   20
///    / \  |   |  / \   |        /  \         |
///   0  1  3   5 7   8  11      15  16       19
///
/// Expected traversal order is indicated by each leaf's number
fn generate_tree() -> DarkTreeResult<(DarkTree<i32>, Vec<i32>)> {
    let mut tree = DarkTree::new(
        22,
        vec![
            DarkTree::new(
                10,
                vec![
                    DarkTree::new(
                        2,
                        vec![
                            DarkTree::new(0, vec![], None, None),
                            DarkTree::new(1, vec![], None, None),
                        ],
                        None,
                        None,
                    ),
                    DarkTree::new(4, vec![DarkTree::new(3, vec![], None, None)], None, None),
                    DarkTree::new(6, vec![DarkTree::new(5, vec![], None, None)], None, None),
                    DarkTree::new(
                        9,
                        vec![
                            DarkTree::new(7, vec![], None, None),
                            DarkTree::new(8, vec![], None, None),
                        ],
                        None,
                        None,
                    ),
                ],
                None,
                None,
            ),
            DarkTree::new(
                14,
                vec![
                    DarkTree::new(12, vec![DarkTree::new(11, vec![], None, None)], None, None),
                    DarkTree::new(13, vec![], None, None),
                ],
                None,
                None,
            ),
            DarkTree::new(
                21,
                vec![
                    DarkTree::new(
                        17,
                        vec![
                            DarkTree::new(15, vec![], None, None),
                            DarkTree::new(16, vec![], None, None),
                        ],
                        None,
                        None,
                    ),
                    DarkTree::new(18, vec![], None, None),
                    DarkTree::new(20, vec![DarkTree::new(19, vec![], None, None)], None, None),
                ],
                None,
                None,
            ),
        ],
        None,
        None,
    );

    tree.build()?;

    let traversal_order = (0..23).collect();

    Ok((tree, traversal_order))
}

#[test]
pub fn test_darktree_iterator() -> DarkTreeResult<()> {
    let (tree, traversal_order) = generate_tree()?;

    // Use [`DarkTree`] iterator to collect current
    // data, in order
    let nums: Vec<i32> = tree.iter().map(|x| x.data).collect();

    // Verify iterator collected the data in the expected
    // traversal order.
    assert_eq!(nums, traversal_order);

    // Verify using iterator indexing methods to retrieve
    // data from it, returns the expected one, as per
    // expected traversal order.
    assert_eq!(tree.iter().nth(1).unwrap().data, traversal_order[1]);

    // Thanks for reading
    Ok(())
}

#[test]
fn test_darktree_traversal_order() -> DarkTreeResult<()> {
    let (mut tree, traversal_order) = generate_tree()?;

    // Loop using the fusion immutable iterator,
    // verifying we grab the correct [`DarkLeaf`]
    // immutable reference, as per expected
    // traversal order.
    let mut index = 0;
    for leaf in &tree {
        assert_eq!(leaf.data, traversal_order[index]);
        index += 1;
    }

    // Loop using the fusion mutable iterator,
    // verifying we grab the correct [`DarkLeaf`]
    // mutable reference, as per expected traversal
    // order.
    index = 0;
    for leaf in &mut tree {
        assert_eq!(leaf.data, traversal_order[index]);
        index += 1;
    }

    // Loop using [`DarkTree`] .iter_mut() mutable
    // iterator, verifying we grab the correct [`DarkLeaf`]
    // mutable reference, as per expected traversal
    // order.
    for (index, leaf) in tree.iter_mut().enumerate() {
        assert_eq!(leaf.data, traversal_order[index]);
    }

    // Loop using [`DarkTree`] .iter() immutable
    // iterator, verifying we grab the correct [`DarkLeaf`]
    // immutable reference, as per expected traversal
    // order.
    for (index, leaf) in tree.iter().enumerate() {
        assert_eq!(leaf.data, traversal_order[index]);
    }

    // Loop using [`DarkTree`] .into_iter() iterator,
    // which consumes (moves) the tree, verifying we
    // collect the correct [`DarkLeaf`], as per expected
    // traversal order.
    for (index, leaf) in tree.into_iter().enumerate() {
        assert_eq!(leaf.data, traversal_order[index]);
    }

    // Thanks for reading
    Ok(())
}

#[test]
fn test_darktree_mut_iterator() -> DarkTreeResult<()> {
    let (mut tree, _) = generate_tree()?;

    // Loop using [`DarkTree`] .iter_mut() mutable
    // iterator, grabing a mutable reference over a
    // [`DarkLeaf`], and mutating its inner data.
    for leaf in tree.iter_mut() {
        leaf.data += 1;
    }

    // Loop using the fusion mutable iterator,
    // grabing a mutable reference over a
    // [`DarkLeaf`], and mutating its inner data.
    for leaf in &mut tree {
        leaf.data += 1;
    }

    // Verify performed mutation actually happened
    // on original tree. Additionally we verify all
    // indexes are the expected ones.
    assert_eq!(
        tree,
        DarkTree {
            leaf: DarkLeaf {
                data: 24,
                index: 22,
                parent_index: None,
                children_indexes: vec![10, 14, 21]
            },
            children: vec![
                DarkTree {
                    leaf: DarkLeaf {
                        data: 12,
                        index: 10,
                        parent_index: Some(22),
                        children_indexes: vec![2, 4, 6, 9]
                    },
                    children: vec![
                        DarkTree {
                            leaf: DarkLeaf {
                                data: 4,
                                index: 2,
                                parent_index: Some(10),
                                children_indexes: vec![0, 1]
                            },
                            children: vec![
                                DarkTree {
                                    leaf: DarkLeaf {
                                        data: 2,
                                        index: 0,
                                        parent_index: Some(2),
                                        children_indexes: vec![]
                                    },
                                    children: vec![],
                                    min_capacity: 1,
                                    max_capacity: None,
                                },
                                DarkTree {
                                    leaf: DarkLeaf {
                                        data: 3,
                                        index: 1,
                                        parent_index: Some(2),
                                        children_indexes: vec![]
                                    },
                                    children: vec![],
                                    min_capacity: 1,
                                    max_capacity: None,
                                },
                            ],
                            min_capacity: 1,
                            max_capacity: None,
                        },
                        DarkTree {
                            leaf: DarkLeaf {
                                data: 6,
                                index: 4,
                                parent_index: Some(10),
                                children_indexes: vec![3]
                            },
                            children: vec![DarkTree {
                                leaf: DarkLeaf {
                                    data: 5,
                                    index: 3,
                                    parent_index: Some(4),
                                    children_indexes: vec![]
                                },
                                children: vec![],
                                min_capacity: 1,
                                max_capacity: None,
                            },],
                            min_capacity: 1,
                            max_capacity: None,
                        },
                        DarkTree {
                            leaf: DarkLeaf {
                                data: 8,
                                index: 6,
                                parent_index: Some(10),
                                children_indexes: vec![5]
                            },
                            children: vec![DarkTree {
                                leaf: DarkLeaf {
                                    data: 7,
                                    index: 5,
                                    parent_index: Some(6),
                                    children_indexes: vec![]
                                },
                                children: vec![],
                                min_capacity: 1,
                                max_capacity: None,
                            },],
                            min_capacity: 1,
                            max_capacity: None,
                        },
                        DarkTree {
                            leaf: DarkLeaf {
                                data: 11,
                                index: 9,
                                parent_index: Some(10),
                                children_indexes: vec![7, 8]
                            },
                            children: vec![
                                DarkTree {
                                    leaf: DarkLeaf {
                                        data: 9,
                                        index: 7,
                                        parent_index: Some(9),
                                        children_indexes: vec![]
                                    },
                                    children: vec![],
                                    min_capacity: 1,
                                    max_capacity: None,
                                },
                                DarkTree {
                                    leaf: DarkLeaf {
                                        data: 10,
                                        index: 8,
                                        parent_index: Some(9),
                                        children_indexes: vec![]
                                    },
                                    children: vec![],
                                    min_capacity: 1,
                                    max_capacity: None,
                                },
                            ],
                            min_capacity: 1,
                            max_capacity: None,
                        },
                    ],
                    min_capacity: 1,
                    max_capacity: None,
                },
                DarkTree {
                    leaf: DarkLeaf {
                        data: 16,
                        index: 14,
                        parent_index: Some(22),
                        children_indexes: vec![12, 13]
                    },
                    children: vec![
                        DarkTree {
                            leaf: DarkLeaf {
                                data: 14,
                                index: 12,
                                parent_index: Some(14),
                                children_indexes: vec![11]
                            },
                            children: vec![DarkTree {
                                leaf: DarkLeaf {
                                    data: 13,
                                    index: 11,
                                    parent_index: Some(12),
                                    children_indexes: vec![]
                                },
                                children: vec![],
                                min_capacity: 1,
                                max_capacity: None,
                            },],
                            min_capacity: 1,
                            max_capacity: None,
                        },
                        DarkTree {
                            leaf: DarkLeaf {
                                data: 15,
                                index: 13,
                                parent_index: Some(14),
                                children_indexes: vec![]
                            },
                            children: vec![],
                            min_capacity: 1,
                            max_capacity: None,
                        },
                    ],
                    min_capacity: 1,
                    max_capacity: None,
                },
                DarkTree {
                    leaf: DarkLeaf {
                        data: 23,
                        index: 21,
                        parent_index: Some(22),
                        children_indexes: vec![17, 18, 20]
                    },
                    children: vec![
                        DarkTree {
                            leaf: DarkLeaf {
                                data: 19,
                                index: 17,
                                parent_index: Some(21),
                                children_indexes: vec![15, 16]
                            },
                            children: vec![
                                DarkTree {
                                    leaf: DarkLeaf {
                                        data: 17,
                                        index: 15,
                                        parent_index: Some(17),
                                        children_indexes: vec![]
                                    },
                                    children: vec![],
                                    min_capacity: 1,
                                    max_capacity: None,
                                },
                                DarkTree {
                                    leaf: DarkLeaf {
                                        data: 18,
                                        index: 16,
                                        parent_index: Some(17),
                                        children_indexes: vec![]
                                    },
                                    children: vec![],
                                    min_capacity: 1,
                                    max_capacity: None,
                                },
                            ],
                            min_capacity: 1,
                            max_capacity: None,
                        },
                        DarkTree {
                            leaf: DarkLeaf {
                                data: 20,
                                index: 18,
                                parent_index: Some(21),
                                children_indexes: vec![]
                            },
                            children: vec![],
                            min_capacity: 1,
                            max_capacity: None,
                        },
                        DarkTree {
                            leaf: DarkLeaf {
                                data: 22,
                                index: 20,
                                parent_index: Some(21),
                                children_indexes: vec![19]
                            },
                            children: vec![DarkTree {
                                leaf: DarkLeaf {
                                    data: 21,
                                    index: 19,
                                    parent_index: Some(20),
                                    children_indexes: vec![]
                                },
                                children: vec![],
                                min_capacity: 1,
                                max_capacity: None,
                            },],
                            min_capacity: 1,
                            max_capacity: None,
                        },
                    ],
                    min_capacity: 1,
                    max_capacity: None,
                },
            ],
            min_capacity: 1,
            max_capacity: None,
        }
    );

    let traversal_order: Vec<i32> = (2..25).collect();

    // Use [`DarkTree`] iterator to collect current
    // data, in order
    let nums: Vec<i32> = tree.iter().map(|x| x.data).collect();

    // Verify iterator collected the data in the expected
    // traversal order.
    assert_eq!(nums, traversal_order);

    // Thanks for reading
    Ok(())
}

#[test]
pub fn test_darktree_min_capacity() -> DarkTreeResult<()> {
    // Generate a new [`DarkTree`] with min capacity 0
    let mut tree = DarkTree::new(0, vec![], Some(0), None);

    // Verify that min capacity was properly setup to 1
    assert_eq!(
        tree,
        DarkTree {
            leaf: DarkLeaf { data: 0, index: 0, parent_index: None, children_indexes: vec![] },
            children: vec![],
            min_capacity: 1,
            max_capacity: None
        }
    );

    // Verify that building it will succeed, as capacity
    // would have ben setup to 1
    assert!(tree.build().is_ok());

    // Generate a new [`DarkTree`] manually with
    // min capacity 0
    let mut tree = DarkTree {
        leaf: DarkLeaf { data: 0, index: 0, parent_index: None, children_indexes: vec![] },
        children: vec![],
        min_capacity: 0,
        max_capacity: None,
    };

    // Verify that building it will fail
    assert!(tree.build().is_err());

    // Thanks for reading
    Ok(())
}

#[test]
pub fn test_darktree_max_capacity() -> DarkTreeResult<()> {
    // Generate a new [`DarkTree`] with max capacity 2
    let mut tree = DarkTree::new(0, vec![], None, Some(2));

    // Append a new node
    tree.append(DarkTree::new(1, vec![], None, None))?;

    // Try to append a new node
    assert!(tree.append(DarkTree::new(2, vec![], None, None)).is_err());

    // Verify tree builds
    tree.build()?;

    // Generate a new [`DarkTree`] with max capacity 2
    let mut new_tree = DarkTree::new(3, vec![], None, Some(2));

    // Append the previous tree as a new node
    new_tree.append(tree)?;

    // Check that max capacity has been exceeded
    assert!(new_tree.check_max_capacity().is_err());

    // Generate a new [`DarkTree`] manually with
    // max capacity 1
    let mut tree = DarkTree {
        leaf: DarkLeaf { data: 0, index: 0, parent_index: None, children_indexes: vec![] },
        children: vec![
            DarkTree {
                leaf: DarkLeaf { data: 0, index: 0, parent_index: None, children_indexes: vec![] },
                children: vec![],
                min_capacity: 1,
                max_capacity: None,
            },
            DarkTree {
                leaf: DarkLeaf { data: 0, index: 0, parent_index: None, children_indexes: vec![] },
                children: vec![],
                min_capacity: 1,
                max_capacity: None,
            },
            DarkTree {
                leaf: DarkLeaf { data: 0, index: 0, parent_index: None, children_indexes: vec![0] },
                children: vec![],
                min_capacity: 1,
                max_capacity: None,
            },
        ],
        min_capacity: 1,
        max_capacity: Some(1),
    };

    // Verify that building it will fail
    assert!(tree.build().is_err());

    // Generate a new [`DarkTree`] with max capacity 0,
    // which is less that current min capacity 1
    let mut tree = DarkTree::new(0, vec![], None, Some(0));

    // Verify that building it will fail
    assert!(tree.build().is_err());

    // Thanks for reading
    Ok(())
}

#[test]
pub fn test_darktree_flattened_vec() -> DarkTreeResult<()> {
    let (mut tree, traversal_order) = generate_tree()?;

    // Build the flattened vector
    let vec = tree.build_vec()?;

    // Verify vector integrity
    dark_leaf_vec_integrity_check(&vec, Some(23), Some(23))?;

    // Verify vector integrity will fail using different bounds:
    // 1. Leafs less that min capacity
    assert!(dark_leaf_vec_integrity_check(&vec, Some(24), None).is_err());
    // 2. Leafs more than max capacity
    assert!(dark_leaf_vec_integrity_check(&vec, None, Some(22)).is_err());
    // 3. Max capacity less than min capacity
    assert!(dark_leaf_vec_integrity_check(&vec, Some(23), Some(22)).is_err());

    // Loop the vector to verify it follows expected
    // traversal order.
    let mut index = 0;
    for leaf in vec {
        assert_eq!(leaf.data, traversal_order[index]);
        index += 1;
    }

    // Verify the tree is still intact
    let (new_tree, _) = generate_tree()?;
    assert_eq!(tree, new_tree);

    // Generate a new [`DarkLeaf`] vector manually,
    // corresponding to a [`DarkTree`] with a 2 children,
    // with erroneous indexes
    let vec = vec![
        DarkLeaf { data: 0, index: 0, parent_index: Some(2), children_indexes: vec![] },
        DarkLeaf { data: 0, index: 1, parent_index: Some(2), children_indexes: vec![] },
        DarkLeaf { data: 0, index: 2, parent_index: None, children_indexes: vec![0, 2] },
    ];

    // Verify vector integrity will fail
    assert!(dark_leaf_vec_integrity_check(&vec, None, None).is_err());

    // Thanks for reading
    Ok(())
}
