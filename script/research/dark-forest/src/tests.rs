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

fn generate_tree() -> (DarkTree<i32>, Vec<i32>) {
    let tree = DarkTree::new(
        5,
        vec![
            DarkTree::new(4, vec![DarkTree::new(3, vec![]), DarkTree::new(2, vec![])]),
            DarkTree::new(1, vec![DarkTree::new(0, vec![])]),
        ],
    );

    let traversal_order = vec![3, 2, 4, 0, 1, 5];

    (tree, traversal_order)
}

#[test]
pub fn test_darktree_iterator() {
    let (tree, traversal_order) = generate_tree();

    let nums: Vec<i32> = tree.iter().map(|x| x.data).collect();

    assert_eq!(nums, traversal_order);
    assert_eq!(tree.iter().nth(1).unwrap().data, traversal_order[1]);
}

#[test]
fn test_darktree_traversal_order() {
    let (mut tree, traversal_order) = generate_tree();

    let mut index = 0;
    for leaf in &tree {
        assert_eq!(leaf.data, traversal_order[index]);
        index += 1;
    }

    index = 0;
    for leaf in &mut tree {
        assert_eq!(leaf.data, traversal_order[index]);
        index += 1;
    }

    for (index, leaf) in tree.iter_mut().enumerate() {
        assert_eq!(leaf.data, traversal_order[index]);
    }

    for (index, leaf) in tree.iter().enumerate() {
        assert_eq!(leaf.data, traversal_order[index]);
    }

    for (index, leaf) in tree.into_iter().enumerate() {
        assert_eq!(leaf.data, traversal_order[index]);
    }
}

#[test]
fn test_darktree_mut_iterator() {
    let (mut tree, _) = generate_tree();

    for leaf in tree.iter_mut() {
        leaf.data += 1;
    }

    for leaf in &mut tree {
        leaf.data += 1;
    }

    assert_eq!(
        tree,
        DarkTree::new(
            7,
            vec![
                DarkTree::new(6, vec![DarkTree::new(5, vec![]), DarkTree::new(4, vec![]),]),
                DarkTree::new(3, vec![DarkTree::new(2, vec![]),]),
            ]
        )
    );
}
