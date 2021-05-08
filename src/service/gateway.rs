use image::EncodableLayout;

use crate::Result;
use crate::serial::{serialize, deserialize};
use super::reqrep::{Request, Reply};

use async_zmq;
use async_std::sync::Arc;
use async_executor::Executor;
use futures::FutureExt;



pub struct GatewayService;


enum NetEvent{
    RECEIVE(async_zmq::Multipart),
    SEND(async_zmq::Multipart)
}


impl GatewayService {

    pub async fn start(
        executor: Arc<Executor<'_>>,
    ) {
        let mut worker = async_zmq::reply("tcp://127.0.0.1:4444").unwrap().connect().unwrap();

        let (send_queue_s, send_queue_r) = async_channel::unbounded::<async_zmq::Multipart>();

        let ex2 = executor.clone();
        loop {
            let event = futures::select! {
                request = worker.recv().fuse() => NetEvent::RECEIVE(request.unwrap()),
                reply = send_queue_r.recv().fuse() => NetEvent::SEND(reply.unwrap())
            };

            match event {
                NetEvent::RECEIVE(request) => {
                    ex2.spawn(Self::handle_request(send_queue_s.clone(), request)).detach();
                },
                NetEvent::SEND(reply) => {
                    worker.send(reply).await.unwrap();
                },
            }
        }

    }

    async fn handle_request(send_queue: async_channel::Sender<async_zmq::Multipart>, request: async_zmq::Multipart) -> Result<()> {
        let mut messages = vec![];
        for req in request.iter() {
            let req = req.as_bytes();
            let req: Request = deserialize(req).unwrap();

            // TODO
            // do things

            println!("Gateway service received a msg {:?}", req);

            let rep = Reply::from(&req, 0, "text".as_bytes().to_vec());
            let rep = serialize(&rep);
            let msg = async_zmq::Message::from(rep);
            messages.push(msg);
        }
        send_queue.send(messages).await?;
        Ok(())
    }
}


struct GatewayClient;


#[repr(u8)]
enum GatewayCommand{
    PUTSLAB,
    GETSLAB,
    GETLASTINDEX,
}

