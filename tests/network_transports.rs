use async_std::{
    io,
    io::{ReadExt, WriteExt},
    stream::StreamExt,
    task,
};
use url::Url;

use darkfi::net::transport::{TcpTransport, TlsTransport, Transport};

#[async_std::test]
async fn tcp_transport() {
    let tcp = TcpTransport { ttl: None };
    let url = Url::parse("tcp://127.0.0.1:5432").unwrap();

    let listener = tcp.clone().listen_on(url.clone()).unwrap().await.unwrap();

    let _ = task::spawn(async move {
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let (reader, writer) = &mut (&stream, &stream);
            io::copy(reader, writer).await.unwrap();
        }
    });

    let payload = b"ohai tcp";

    let mut client = tcp.dial(url).unwrap().await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0_u8; 8];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf, payload);
}

#[async_std::test]
async fn tls_transport() {
    let tls = TlsTransport { ttl: None };
    let url = Url::parse("tls://127.0.0.1:5433").unwrap();

    let (acceptor, listener) = tls.clone().listen_on(url.clone()).unwrap().await.unwrap();

    let _ = task::spawn(async move {
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let stream = acceptor.accept(stream).await.unwrap();
            let (mut reader, mut writer) = smol::io::split(stream);
            io::copy(&mut reader, &mut writer).await.unwrap();
        }
    });

    let payload = b"ohai tls";

    let mut client = tls.dial(url).unwrap().await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0_u8; 8];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf, payload);
}
