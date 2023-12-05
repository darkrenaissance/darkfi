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

use crate::DarkTree;

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
fn generate_tree() -> (DarkTree<i32>, Vec<i32>) {
    let tree = DarkTree::new(
        22,
        vec![
            DarkTree::new(
                10,
                vec![
                    DarkTree::new(2, vec![DarkTree::new(0, vec![]), DarkTree::new(1, vec![])]),
                    DarkTree::new(4, vec![DarkTree::new(3, vec![])]),
                    DarkTree::new(6, vec![DarkTree::new(5, vec![])]),
                    DarkTree::new(9, vec![DarkTree::new(7, vec![]), DarkTree::new(8, vec![])]),
                ],
            ),
            DarkTree::new(
                14,
                vec![DarkTree::new(12, vec![DarkTree::new(11, vec![])]), DarkTree::new(13, vec![])],
            ),
            DarkTree::new(
                21,
                vec![
                    DarkTree::new(17, vec![DarkTree::new(15, vec![]), DarkTree::new(16, vec![])]),
                    DarkTree::new(18, vec![]),
                    DarkTree::new(20, vec![DarkTree::new(19, vec![])]),
                ],
            ),
        ],
    );

    let traversal_order = (0..23).collect();

    (tree, traversal_order)
}

#[test]
pub fn test_darktree_iterator() {
    let (tree, traversal_order) = generate_tree();

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
}

#[test]
fn test_darktree_traversal_order() {
    let (mut tree, traversal_order) = generate_tree();

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
}

#[test]
fn test_darktree_mut_iterator() {
    let (mut tree, _) = generate_tree();

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
    // on original tree.
    assert_eq!(
        tree,
        DarkTree::new(
            24,
            vec![
                DarkTree::new(
                    12,
                    vec![
                        DarkTree::new(4, vec![DarkTree::new(2, vec![]), DarkTree::new(3, vec![]),]),
                        DarkTree::new(6, vec![DarkTree::new(5, vec![]),]),
                        DarkTree::new(8, vec![DarkTree::new(7, vec![]),]),
                        DarkTree::new(
                            11,
                            vec![DarkTree::new(9, vec![]), DarkTree::new(10, vec![]),]
                        ),
                    ]
                ),
                DarkTree::new(
                    16,
                    vec![
                        DarkTree::new(14, vec![DarkTree::new(13, vec![]),]),
                        DarkTree::new(15, vec![]),
                    ]
                ),
                DarkTree::new(
                    23,
                    vec![
                        DarkTree::new(
                            19,
                            vec![DarkTree::new(17, vec![]), DarkTree::new(18, vec![]),]
                        ),
                        DarkTree::new(20, vec![]),
                        DarkTree::new(22, vec![DarkTree::new(21, vec![]),]),
                    ]
                ),
            ],
        )
    );

    let traversal_order: Vec<i32> = (2..25).collect();

    // Use [`DarkTree`] iterator to collect current
    // data, in order
    let nums: Vec<i32> = tree.iter().map(|x| x.data).collect();

    // Verify iterator collected the data in the expected
    // traversal order.
    assert_eq!(nums, traversal_order);
}
