pub mod event;
pub mod gset;
pub mod net;
pub mod node;

pub use event::Event;
pub use gset::GSet;
pub use net::CrdtP2p;
pub use node::Node;

#[cfg(test)]
mod tests {

    use super::*;

    fn sync_simulation(mut a: Node, mut b: Node, mut c: Node) -> (Node, Node, Node) {
        a.gset.merge(&b.gset);
        a.gset.merge(&c.gset);

        b.gset.merge(&a.gset);
        b.gset.merge(&c.gset);

        c.gset.merge(&a.gset);
        c.gset.merge(&b.gset);

        (a, b, c)
    }

    #[test]
    fn test_crdt_gset() {
        let mut a: Node = Node::new("Node A");
        let mut b: Node = Node::new("Node B");
        let mut c: Node = Node::new("Node C");

        // node a
        a.send_event("a_msg1".to_string());
        a.send_event("a_msg2".to_string());

        // node b
        b.send_event("b_msg1".to_string());

        // node c
        c.send_event("c_msg1".to_string());

        // node b
        b.send_event("b_msg2".to_string());

        let (a, mut b, mut c) = sync_simulation(a, b, c);

        assert_eq!(a.gset.len(), 5);
        assert_eq!(b.gset.len(), 5);
        assert_eq!(c.gset.len(), 5);

        // node c
        c.send_event("c_msg2".to_string());
        c.send_event("c_msg3".to_string());
        c.send_event("c_msg4".to_string());
        c.send_event("c_msg5".to_string());

        // node b
        b.send_event("b_msg3".to_string());

        let (a, b, c) = sync_simulation(a, b, c);

        assert_eq!(a.gset.len(), 10);
        assert_eq!(b.gset.len(), 10);
        assert_eq!(c.gset.len(), 10);
    }
}
