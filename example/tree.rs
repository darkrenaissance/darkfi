use incrementalmerkletree::{bridgetree::BridgeTree, Frontier, Tree};
use pasta_curves::{arithmetic::Field, pallas};
use rand::rngs::OsRng;

use darkfi::{crypto::merkle_node::MerkleNode, Result};

fn main() -> Result<()> {
    let mut tree = BridgeTree::<MerkleNode, 32>::new(100);

    for i in 0..10 {
        tree.append(&MerkleNode(pallas::Base::random(&mut OsRng)));
        if i % 3 == 0 {
            tree.witness();
        }
    }

    let bytes = bincode::serialize(&tree).unwrap();
    let tree2: BridgeTree<MerkleNode, 32> = bincode::deserialize(&bytes).unwrap();

    let root1 = tree.root();
    let root2 = tree2.root();

    assert_eq!(root1, root2);

    Ok(())
}
