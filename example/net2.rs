use async_std::io::{ReadExt, WriteExt};
use darkfi::net::transport::{TcpTransport, TlsTransport, Transport};
use std::{fs::File, io::Write};
use url::Url;

#[async_std::main]
async fn main() {
    // nc -l 127.0.0.1 5432
    // let tcp = TcpTransport { ttl: None };
    // let url = Url::parse("tcp://127.0.0.1:5432").unwrap();

    // let mut socket = tcp.dial(url).unwrap().await.unwrap();
    // socket.write_all(b"ohai tcp\n").await.unwrap();
    // socket.flush().await;

    let tls = TlsTransport { ttl: None };
    let url = Url::parse("tls://parazyd.org:70").unwrap();
    let mut socket = tls.dial(url).unwrap().await.unwrap();
    socket.write_all(b"/rms.png\r\n").await.unwrap();

    let mut buf = vec![];
    socket.read_to_end(&mut buf).await.unwrap();

    let mut file = File::create("rms.png").unwrap();
    file.write_all(&buf).unwrap();
}
