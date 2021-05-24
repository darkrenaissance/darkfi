//! Implementation of a Merkle tree of commitments used to prove the existence
//! of notes.

//use byteorder::{LittleEndian, ReadBytesExt};
use std::collections::VecDeque;
use std::io::{self, Read, Write};

//use crate::serialize::{Optional, Vector};
use super::merkle_node::SAPLING_COMMITMENT_TREE_DEPTH;

/// A hashable node within a Merkle tree.
pub trait Hashable: Clone + Copy {
    /// Parses a node from the given byte source.
    fn read<R: Read>(reader: R) -> io::Result<Self>;

    /// Serializes this node.
    fn write<W: Write>(&self, writer: W) -> io::Result<()>;

    /// Returns the parent node within the tree of the two given nodes.
    fn combine(_: usize, _: &Self, _: &Self) -> Self;

    /// Returns a blank leaf node.
    fn blank() -> Self;

    /// Returns the empty root for the given depth.
    fn empty_root(_: usize) -> Self;
}

struct PathFiller<Node: Hashable> {
    queue: VecDeque<Node>,
}

impl<Node: Hashable> PathFiller<Node> {
    fn empty() -> Self {
        PathFiller {
            queue: VecDeque::new(),
        }
    }

    fn next(&mut self, depth: usize) -> Node {
        self.queue
            .pop_front()
            .unwrap_or_else(|| Node::empty_root(depth))
    }
}

/// A Merkle tree of note commitments.
///
/// The depth of the Merkle tree is fixed at 32, equal to the depth of the
/// Sapling commitment tree.
#[derive(Clone)]
pub struct CommitmentTree<Node: Hashable> {
    left: Option<Node>,
    right: Option<Node>,
    parents: Vec<Option<Node>>,
}

impl<Node: Hashable> CommitmentTree<Node> {
    /// Creates an empty tree.
    pub fn empty() -> Self {
        CommitmentTree {
            left: None,
            right: None,
            parents: vec![],
        }
    }

    /*
    /// Reads a `CommitmentTree` from its serialized form.
    #[allow(clippy::redundant_closure)]
    pub fn read<R: Read>(mut reader: R) -> io::Result<Self> {
        let left = Optional::read(&mut reader, |r| Node::read(r))?;
        let right = Optional::read(&mut reader, |r| Node::read(r))?;
        let parents = Vector::read(&mut reader, |r| Optional::read(r, |r| Node::read(r)))?;

        Ok(CommitmentTree {
            left,
            right,
            parents,
        })
    }

    /// Serializes this tree as an array of bytes.
    pub fn write<W: Write>(&self, mut writer: W) -> io::Result<()> {
        Optional::write(&mut writer, &self.left, |w, n| n.write(w))?;
        Optional::write(&mut writer, &self.right, |w, n| n.write(w))?;
        Vector::write(&mut writer, &self.parents, |w, e| {
            Optional::write(w, e, |w, n| n.write(w))
        })
    }
    */

    /// Returns the number of leaf nodes in the tree.
    pub fn size(&self) -> usize {
        self.parents.iter().enumerate().fold(
            match (self.left, self.right) {
                (None, None) => 0,
                (Some(_), None) => 1,
                (Some(_), Some(_)) => 2,
                (None, Some(_)) => unreachable!(),
            },
            |acc, (i, p)| {
                // Treat occupation of parents array as a binary number
                // (right-shifted by 1)
                acc + if p.is_some() { 1 << (i + 1) } else { 0 }
            },
        )
    }

    fn is_complete(&self, depth: usize) -> bool {
        self.left.is_some()
            && self.right.is_some()
            && self.parents.len() == depth - 1
            && self.parents.iter().all(|p| p.is_some())
    }

    /// Adds a leaf node to the tree.
    ///
    /// Returns an error if the tree is full.
    pub fn append(&mut self, node: Node) -> Result<(), ()> {
        self.append_inner(node, SAPLING_COMMITMENT_TREE_DEPTH)
    }

    fn append_inner(&mut self, node: Node, depth: usize) -> Result<(), ()> {
        if self.is_complete(depth) {
            // Tree is full
            return Err(());
        }

        match (self.left, self.right) {
            (None, _) => self.left = Some(node),
            (_, None) => self.right = Some(node),
            (Some(l), Some(r)) => {
                let mut combined = Node::combine(0, &l, &r);
                self.left = Some(node);
                self.right = None;

                for i in 0..depth {
                    if i < self.parents.len() {
                        if let Some(p) = self.parents[i] {
                            combined = Node::combine(i + 1, &p, &combined);
                            self.parents[i] = None;
                        } else {
                            self.parents[i] = Some(combined);
                            break;
                        }
                    } else {
                        self.parents.push(Some(combined));
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns the current root of the tree.
    pub fn root(&self) -> Node {
        self.root_inner(SAPLING_COMMITMENT_TREE_DEPTH, PathFiller::empty())
    }

    fn root_inner(&self, depth: usize, mut filler: PathFiller<Node>) -> Node {
        assert!(depth > 0);

        // 1) Hash left and right leaves together.
        //    - Empty leaves are used as needed.
        let leaf_root = Node::combine(
            0,
            &self.left.unwrap_or_else(|| filler.next(0)),
            &self.right.unwrap_or_else(|| filler.next(0)),
        );

        // 2) Hash in parents up to the currently-filled depth.
        //    - Roots of the empty subtrees are used as needed.
        let mid_root = self
            .parents
            .iter()
            .enumerate()
            .fold(leaf_root, |root, (i, p)| match p {
                Some(node) => Node::combine(i + 1, node, &root),
                None => Node::combine(i + 1, &root, &filler.next(i + 1)),
            });

        // 3) Hash in roots of the empty subtrees up to the final depth.
        ((self.parents.len() + 1)..depth)
            .fold(mid_root, |root, d| Node::combine(d, &root, &filler.next(d)))
    }
}

/// An updatable witness to a path from a position in a particular
/// [`CommitmentTree`].
///
/// Appending the same commitments in the same order to both the original
/// [`CommitmentTree`] and this `IncrementalWitness` will result in a witness to
/// the path from the target position to the root of the updated tree.
///
/// # Examples
///
/// ```
/// use ff::{Field, PrimeField};
/// use rand_core::OsRng;
/// use zcash_primitives::{
///     merkle_tree::{CommitmentTree, IncrementalWitness},
///     sapling::Node,
/// };
///
/// let mut rng = OsRng;
///
/// let mut tree = CommitmentTree::<Node>::empty();
///
/// tree.append(Node::new(bls12_381::Scalar::random(&mut rng).to_repr()));
/// tree.append(Node::new(bls12_381::Scalar::random(&mut rng).to_repr()));
/// let mut witness = IncrementalWitness::from_tree(&tree);
/// assert_eq!(witness.position(), 1);
/// assert_eq!(tree.root(), witness.root());
///
/// let cmu = Node::new(bls12_381::Scalar::random(&mut rng).to_repr());
/// tree.append(cmu);
/// witness.append(cmu);
/// assert_eq!(tree.root(), witness.root());
/// ```
#[derive(Clone)]
pub struct IncrementalWitness<Node: Hashable> {
    tree: CommitmentTree<Node>,
    filled: Vec<Node>,
    cursor_depth: usize,
    cursor: Option<CommitmentTree<Node>>,
}

impl<Node: Hashable> IncrementalWitness<Node> {
    /// Creates an `IncrementalWitness` for the most recent commitment added to
    /// the given [`CommitmentTree`].
    pub fn from_tree(tree: &CommitmentTree<Node>) -> IncrementalWitness<Node> {
        IncrementalWitness {
            tree: tree.clone(),
            filled: vec![],
            cursor_depth: 0,
            cursor: None,
        }
    }

    /*
    /// Reads an `IncrementalWitness` from its serialized form.
    #[allow(clippy::redundant_closure)]
    pub fn read<R: Read>(mut reader: R) -> io::Result<Self> {
        let tree = CommitmentTree::read(&mut reader)?;
        let filled = Vector::read(&mut reader, |r| Node::read(r))?;
        let cursor = Optional::read(&mut reader, |r| CommitmentTree::read(r))?;

        let mut witness = IncrementalWitness {
            tree,
            filled,
            cursor_depth: 0,
            cursor,
        };

        witness.cursor_depth = witness.next_depth();

        Ok(witness)
    }

    /// Serializes this `IncrementalWitness` as an array of bytes.
    pub fn write<W: Write>(&self, mut writer: W) -> io::Result<()> {
        self.tree.write(&mut writer)?;
        Vector::write(&mut writer, &self.filled, |w, n| n.write(w))?;
        Optional::write(&mut writer, &self.cursor, |w, t| t.write(w))
    }
    */

    /// Returns the position of the witnessed leaf node in the commitment tree.
    pub fn position(&self) -> usize {
        self.tree.size() - 1
    }

    fn filler(&self) -> PathFiller<Node> {
        let cursor_root = self
            .cursor
            .as_ref()
            .map(|c| c.root_inner(self.cursor_depth, PathFiller::empty()));

        PathFiller {
            queue: self.filled.iter().cloned().chain(cursor_root).collect(),
        }
    }

    /// Finds the next "depth" of an unfilled subtree.
    fn next_depth(&self) -> usize {
        let mut skip = self.filled.len();

        if self.tree.left.is_none() {
            if skip > 0 {
                skip -= 1;
            } else {
                return 0;
            }
        }

        if self.tree.right.is_none() {
            if skip > 0 {
                skip -= 1;
            } else {
                return 0;
            }
        }

        let mut d = 1;
        for p in &self.tree.parents {
            if p.is_none() {
                if skip > 0 {
                    skip -= 1;
                } else {
                    return d;
                }
            }
            d += 1;
        }

        d + skip
    }

    /// Tracks a leaf node that has been added to the underlying tree.
    ///
    /// Returns an error if the tree is full.
    pub fn append(&mut self, node: Node) -> Result<(), ()> {
        self.append_inner(node, SAPLING_COMMITMENT_TREE_DEPTH)
    }

    fn append_inner(&mut self, node: Node, depth: usize) -> Result<(), ()> {
        if let Some(mut cursor) = self.cursor.take() {
            cursor
                .append_inner(node, depth)
                .expect("cursor should not be full");
            if cursor.is_complete(self.cursor_depth) {
                self.filled
                    .push(cursor.root_inner(self.cursor_depth, PathFiller::empty()));
            } else {
                self.cursor = Some(cursor);
            }
        } else {
            self.cursor_depth = self.next_depth();
            if self.cursor_depth >= depth {
                // Tree is full
                return Err(());
            }

            if self.cursor_depth == 0 {
                self.filled.push(node);
            } else {
                let mut cursor = CommitmentTree::empty();
                cursor
                    .append_inner(node, depth)
                    .expect("cursor should not be full");
                self.cursor = Some(cursor);
            }
        }

        Ok(())
    }

    /// Returns the current root of the tree corresponding to the witness.
    pub fn root(&self) -> Node {
        self.root_inner(SAPLING_COMMITMENT_TREE_DEPTH)
    }

    fn root_inner(&self, depth: usize) -> Node {
        self.tree.root_inner(depth, self.filler())
    }

    /// Returns the current witness, or None if the tree is empty.
    pub fn path(&self) -> Option<MerklePath<Node>> {
        self.path_inner(SAPLING_COMMITMENT_TREE_DEPTH)
    }

    fn path_inner(&self, depth: usize) -> Option<MerklePath<Node>> {
        let mut filler = self.filler();
        let mut auth_path = Vec::new();

        if let Some(node) = self.tree.left {
            if self.tree.right.is_some() {
                auth_path.push((node, true));
            } else {
                auth_path.push((filler.next(0), false));
            }
        } else {
            // Can't create an authentication path for the beginning of the tree
            return None;
        }

        for (i, p) in self.tree.parents.iter().enumerate() {
            auth_path.push(match p {
                Some(node) => (*node, true),
                None => (filler.next(i + 1), false),
            });
        }

        for i in self.tree.parents.len()..(depth - 1) {
            auth_path.push((filler.next(i + 1), false));
        }
        assert_eq!(auth_path.len(), depth);

        Some(MerklePath::from_path(auth_path, self.position() as u64))
    }
}

/// A path from a position in a particular commitment tree to the root of that
/// tree.
#[derive(Clone, Debug, PartialEq)]
pub struct MerklePath<Node: Hashable> {
    pub auth_path: Vec<(Node, bool)>,
    pub position: u64,
}

impl<Node: Hashable> MerklePath<Node> {
    /// Constructs a Merkle path directly from a path and position.
    pub fn from_path(auth_path: Vec<(Node, bool)>, position: u64) -> Self {
        MerklePath {
            auth_path,
            position,
        }
    }

    /*
    /// Reads a Merkle path from its serialized form.
    pub fn from_slice(witness: &[u8]) -> Result<Self, ()> {
        Self::from_slice_with_depth(witness, SAPLING_COMMITMENT_TREE_DEPTH)
    }

    fn from_slice_with_depth(mut witness: &[u8], depth: usize) -> Result<Self, ()> {
        // Skip the first byte, which should be "depth" to signify the length of
        // the following vector of Pedersen hashes.
        if witness[0] != depth as u8 {
            return Err(());
        }
        witness = &witness[1..];

        // Begin to construct the authentication path
        let iter = witness.chunks_exact(33);
        witness = iter.remainder();

        // The vector works in reverse
        let mut auth_path = iter
            .rev()
            .map(|bytes| {
                // Length of inner vector should be the length of a Pedersen hash
                if bytes[0] == 32 {
                    // Sibling node should be an element of Fr
                    Node::read(&bytes[1..])
                        .map(|sibling| {
                            // Set the value in the auth path; we put false here
                            // for now (signifying the position bit) which we'll
                            // fill in later.
                            (sibling, false)
                        })
                        .map_err(|_| ())
                } else {
                    Err(())
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if auth_path.len() != depth {
            return Err(());
        }

        // Read the position from the witness
        let position = witness.read_u64::<LittleEndian>().map_err(|_| ())?;

        // Given the position, let's finish constructing the authentication
        // path
        let mut tmp = position;
        for entry in auth_path.iter_mut() {
            entry.1 = (tmp & 1) == 1;
            tmp >>= 1;
        }

        // The witness should be empty now; if it wasn't, the caller would
        // have provided more information than they should have, indicating
        // a bug downstream
        if witness.is_empty() {
            Ok(MerklePath {
                auth_path,
                position,
            })
        } else {
            Err(())
        }
    }
    */

    /// Returns the root of the tree corresponding to this path applied to
    /// `leaf`.
    pub fn root(&self, leaf: Node) -> Node {
        self.auth_path
            .iter()
            .enumerate()
            .fold(
                leaf,
                |root, (i, (p, leaf_is_on_right))| match leaf_is_on_right {
                    false => Node::combine(i, &root, p),
                    true => Node::combine(i, p, &root),
                },
            )
    }
}
