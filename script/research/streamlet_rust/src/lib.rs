pub mod structures;

#[cfg(test)]
mod tests {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use super::structures::{block::Block, node::Node};

    use darkfi::{crypto::token_id::generate_id2, util::NetworkName};

    #[test]
    fn protocol_execution() {
        // Genesis block is generated.
        let mut genesis_block = Block::new(
            String::from("⊥"),
            0,
            vec![],
            String::from("proof"),
            String::from("r"),
            String::from("s"),
        );
        genesis_block.metadata.sm.notarized = true;
        genesis_block.metadata.sm.finalized = true;

        let genesis_time = Instant::now();

        // We create some nodes to participate in the Protocol.
        let mut node0 = Node::new(0, genesis_time, genesis_block.clone());
        let mut node1 = Node::new(1, genesis_time, genesis_block.clone());
        let mut node2 = Node::new(2, genesis_time, genesis_block.clone());

        // We store nodes public keys for voting.
        let node0_public_key = node0.public_key;
        let node1_public_key = node1.public_key;
        let node2_public_key = node2.public_key;

        // We simulate some epochs to test consistency.
        let token_id = generate_id2("STREAMLET", &NetworkName::Ethereum).unwrap();

        let tx = node0.generate_transaction(token_id, 100, &node1_public_key).unwrap();
        node0.receive_transaction(tx.clone());
        node0.broadcast_transaction(vec![&mut node1, &mut node2], tx);
        let tx = node1.generate_transaction(token_id, 200, &node2_public_key).unwrap();
        node1.receive_transaction(tx.clone());
        node1.broadcast_transaction(vec![&mut node0, &mut node2], tx);
        let tx = node2.generate_transaction(token_id, 300, &node1_public_key).unwrap();
        node2.receive_transaction(tx.clone());
        node2.broadcast_transaction(vec![&mut node0, &mut node1], tx);

        // Each node checks if they are the epoch leader. Leader will propose the block.
        let (leader_public_key, block_proposal) = if node0.check_if_epoch_leader(3) {
            node0.propose_block()
        } else if node1.check_if_epoch_leader(3) {
            node1.propose_block()
        } else {
            node2.propose_block()
        };

        // Leader broadcasts the proposed_block to rest nodes and they vote on it.
        let node0_vote =
            node0.receive_proposed_block(&leader_public_key, &block_proposal, 3).unwrap();
        let node1_vote =
            node1.receive_proposed_block(&leader_public_key, &block_proposal, 3).unwrap();
        let node2_vote =
            node2.receive_proposed_block(&leader_public_key, &block_proposal, 3).unwrap();

        // Each node broadcasts its vote to rest nodes.
        node0.receive_vote(&node0_public_key, &node0_vote, 3);
        node0.receive_vote(&node1_public_key, &node1_vote, 3);
        node0.receive_vote(&node2_public_key, &node2_vote, 3);
        node1.receive_vote(&node0_public_key, &node0_vote, 3);
        node1.receive_vote(&node1_public_key, &node1_vote, 3);
        node1.receive_vote(&node2_public_key, &node2_vote, 3);
        node2.receive_vote(&node0_public_key, &node0_vote, 3);
        node2.receive_vote(&node1_public_key, &node1_vote, 3);
        node2.receive_vote(&node2_public_key, &node2_vote, 3);

        // We verify that all nodes have the same blockchain on round end.
        verify_outputs(&node0, &node1, &node2);

        // We use thread sleep to simulate sinchronization period.
        thread::sleep(Duration::new(5, 0));

        // Next round.
        let tx = node0.generate_transaction(token_id, 400, &node1_public_key).unwrap();
        node0.receive_transaction(tx.clone());
        node0.broadcast_transaction(vec![&mut node1, &mut node2], tx);
        let tx = node1.generate_transaction(token_id, 500, &node2_public_key).unwrap();
        node1.receive_transaction(tx.clone());
        node1.broadcast_transaction(vec![&mut node0, &mut node2], tx);
        //let tx = node2.generate_transaction(token_id, 600, &node1_public_key).unwrap();
        //node2.receive_transaction(tx.clone());
        //node2.broadcast_transaction(vec![&mut node0, &mut node1], tx);

        // Each node checks if they are the epoch leader. Leader will propose the block.
        let (leader_public_key, block_proposal) = if node0.check_if_epoch_leader(3) {
            node0.propose_block()
        } else if node1.check_if_epoch_leader(3) {
            node1.propose_block()
        } else {
            node2.propose_block()
        };

        // Leader broadcasts the proposed_block to rest nodes and they vote on it.
        let node0_vote =
            node0.receive_proposed_block(&leader_public_key, &block_proposal, 3).unwrap();
        let node1_vote =
            node1.receive_proposed_block(&leader_public_key, &block_proposal, 3).unwrap();
        let node2_vote =
            node2.receive_proposed_block(&leader_public_key, &block_proposal, 3).unwrap();

        // Each node broadcasts its vote to rest nodes.
        node0.receive_vote(&node0_public_key, &node0_vote, 3);
        node0.receive_vote(&node1_public_key, &node1_vote, 3);
        node0.receive_vote(&node2_public_key, &node2_vote, 3);
        node1.receive_vote(&node0_public_key, &node0_vote, 3);
        node1.receive_vote(&node1_public_key, &node1_vote, 3);
        node1.receive_vote(&node2_public_key, &node2_vote, 3);
        node2.receive_vote(&node0_public_key, &node0_vote, 3);
        node2.receive_vote(&node1_public_key, &node1_vote, 3);
        node2.receive_vote(&node2_public_key, &node2_vote, 3);

        // We verify that all nodes have the same blockchain on round end.
        verify_outputs(&node0, &node1, &node2);

        // We use thread sleep to simulate sinchronization period.
        thread::sleep(Duration::new(5, 0));

        // Next round.
        let tx = node0.generate_transaction(token_id, 700, &node1_public_key).unwrap();
        node0.receive_transaction(tx.clone());
        node0.broadcast_transaction(vec![&mut node1, &mut node2], tx);
        //let tx = node1.generate_transaction(token_id, 800, &node2_public_key).unwrap();
        //node1.receive_transaction(tx.clone());
        //node1.broadcast_transaction(vec![&mut node0, &mut node2], tx);
        //let tx = node2.generate_transaction(token_id, 900, &node1_public_key).unwrap();
        //node2.receive_transaction(tx.clone());
        //node2.broadcast_transaction(vec![&mut node0, &mut node1], tx);

        // Each node checks if they are the epoch leader. Leader will propose the block.
        let (leader_public_key, block_proposal) = if node0.check_if_epoch_leader(3) {
            node0.propose_block()
        } else if node1.check_if_epoch_leader(3) {
            node1.propose_block()
        } else {
            node2.propose_block()
        };

        // Leader broadcasts the proposed_block to rest nodes and they vote on it.
        let node0_vote =
            node0.receive_proposed_block(&leader_public_key, &block_proposal, 3).unwrap();
        let node1_vote =
            node1.receive_proposed_block(&leader_public_key, &block_proposal, 3).unwrap();
        let node2_vote =
            node2.receive_proposed_block(&leader_public_key, &block_proposal, 3).unwrap();

        // Each node broadcasts its vote to rest nodes.
        node0.receive_vote(&node0_public_key, &node0_vote, 3);
        node0.receive_vote(&node1_public_key, &node1_vote, 3);
        node0.receive_vote(&node2_public_key, &node2_vote, 3);
        node1.receive_vote(&node0_public_key, &node0_vote, 3);
        node1.receive_vote(&node1_public_key, &node1_vote, 3);
        node1.receive_vote(&node2_public_key, &node2_vote, 3);
        node2.receive_vote(&node0_public_key, &node0_vote, 3);
        node2.receive_vote(&node1_public_key, &node1_vote, 3);
        node2.receive_vote(&node2_public_key, &node2_vote, 3);

        // We verify that all nodes have the same blockchain on round end.
        verify_outputs(&node0, &node1, &node2);
    }

    fn verify_outputs(node0: &Node, node1: &Node, node2: &Node) {
        assert!(node0.output() == node1.output());
        assert!(node1.output() == node2.output());
    }
}
