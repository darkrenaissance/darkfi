use async_std::io::WriteExt;
use darkfi::net::transport::{TcpTransport, Transport};
use url::Url;

#[async_std::main]
async fn main() {
    let tcp = TcpTransport { ttl: None };
    let url = Url::parse("tcp://127.0.0.1:5432").unwrap();

    let mut socket = tcp.dial(url).unwrap().await.unwrap();
    socket.write_all(b"ohai\n").await.unwrap();
    // socket.flush().await;
}
