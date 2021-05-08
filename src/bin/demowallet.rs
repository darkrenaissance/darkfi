
//! cargo run --example request --features="rt-tokio" --no-default-features

use async_zmq::zmq;
use sapvi::service::reqrep::{Request, Reply};
use sapvi::serial;


fn connect () {
    let context = zmq::Context::new();
    let requester = context.socket(zmq::REQ).unwrap();
    requester
        .connect("tcp://127.0.0.1:3333")
        .expect("failed to connect requester");

    for request_nbr in 0..10 {
        let req = Request::new(0, "test".as_bytes().to_vec());
        let req = serial::serialize(&req);
        requester.send(req, 0).unwrap();
        let message = requester.recv_msg(0).unwrap();
        let rep: Reply = serial::deserialize(&message).unwrap();
        println!(
            "Received reply {:?} {:?}",
            request_nbr,
            rep
        );
    }
}
fn main() {


    let mut thread_pools = vec![];
    for _ in 0..20 {
        let t = std::thread::spawn(connect);
        thread_pools.push(t);
    }

    for t in thread_pools {
        t.join().unwrap();
    }

}
