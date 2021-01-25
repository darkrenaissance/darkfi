use smol::{Async};
use std::net::{TcpStream};

pub struct Proxy {
    stream: Async<TcpStream>,
}

impl Proxy {
    pub fn new(stream: Async<TcpStream>) -> Self {
        Self { stream }
    }
}
