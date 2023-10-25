# merkle-tree

append only merkle-tree `tree`

## merkle-node

a node `node` is a field element on the elliptic curve

## merkle root

hash of leaf up to certain depth root(tree, depth), hash the leafs including the empty nodes up to given `depth`,

## witness

authentication path to given `depth`, and bridge frontier, or position.

## sparse merkle tree

is a merkle-tree with leafs stored in a search tree, has advantage over merkle-tree that is a allow non-inclusion proof, through membership proof to index of data in the search tree.

### membership proof

given `index` the proof is a `path` from the leaf at `index` to the root.
