# Merkle-tree

Append only merkle-tree `tree`

## Merkle-node

A node `node` is a field element on the elliptic curve

## Merkle root

Hash of leaf up to certain depth root(tree, depth), hash the leafs including the empty nodes up to given `depth`,

## Witness

Authentication path to given `depth`, and bridge frontier, or position.

## Sparse merkle tree

Is a merkle-tree with leafs stored in a search tree, has advantage over merkle-tree that is a allow non-inclusion proof, through membership proof to index of data in the search tree.

### Membership proof

Given `index` the proof is a `path` from the leaf at `index` to the root.
