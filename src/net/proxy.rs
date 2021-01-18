use smol::{Async, Executor};
use std::net::{SocketAddr, TcpStream};

pub struct Proxy {
    stream: Async<TcpStream>,
}

impl Proxy {
    pub fn new(stream: Async<TcpStream>) -> Self {
        Self { stream }
    }
}
