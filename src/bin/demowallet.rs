use sapvi::service::reqrep::{Reply, Request};
use sapvi::{serial, Result};

use bytes::Bytes;
use zeromq::*;

async fn connect() -> Result<()> {
    let mut requester = zeromq::ReqSocket::new();
    requester.connect("tcp://127.0.0.1:3333").await?;
    
    println!("connected") ;

    for request_nbr in 0..10 {
        println!("start sending");
        let req = Request::new(0, "test".as_bytes().to_vec());
        let req = serial::serialize(&req);
        let req = bytes::Bytes::from(req);
        requester.send(req.into()).await?;
        let message: zeromq::ZmqMessage = requester.recv().await?;
        let message: &Bytes = message.get(0).unwrap();
        let message: Vec<u8> = message.to_vec();
        let rep: Reply = serial::deserialize(&message).unwrap();
        println!("Received reply {:?} {:?}", request_nbr, rep);
    }
    Ok(())
}

fn main() {
    futures::executor::block_on(connect()).unwrap();
}
