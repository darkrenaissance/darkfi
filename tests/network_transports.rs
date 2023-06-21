/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{env::var, fs};

use async_std::{
    io,
    io::{ReadExt, WriteExt},
    stream::StreamExt,
    task,
};
use url::Url;

use darkfi::net::transport::{NymTransport, TcpTransport, TorTransport, Transport, UnixTransport};

#[async_std::test]
async fn unix_transport() {
    let unix = UnixTransport::new();
    let url = Url::parse("unix:///tmp/darkfi_test.sock").unwrap();

    let listener = unix.listen_on(url.clone()).unwrap().await.unwrap();

    task::spawn(async move {
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let (reader, writer) = &mut (&stream, &stream);
            io::copy(reader, writer).await.unwrap();
        }
    });

    let payload = b"ohai unix";

    let mut client = unix.dial(url, None).unwrap().await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0_u8; 9];
    client.read_exact(&mut buf).await.unwrap();

    std::fs::remove_file("/tmp/darkfi_test.sock").unwrap();
    assert_eq!(buf, payload);
}

#[async_std::test]
async fn tcp_transport() {
    let tcp = TcpTransport::new(None, 1024);
    let url = Url::parse("tcp://127.0.0.1:5432").unwrap();

    let listener = tcp.listen_on(url.clone()).unwrap().await.unwrap();

    task::spawn(async move {
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let (reader, writer) = &mut (&stream, &stream);
            io::copy(reader, writer).await.unwrap();
        }
    });

    let payload = b"ohai tcp";

    let mut client = tcp.dial(url, None).unwrap().await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0_u8; 8];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf, payload);
}

#[async_std::test]
async fn tcp_tls_transport() {
    let tcp = TcpTransport::new(None, 1024);
    let url = Url::parse("tcp+tls://127.0.0.1:5433").unwrap();

    let listener = tcp.listen_on(url.clone()).unwrap().await.unwrap();
    let (acceptor, listener) = tcp.upgrade_listener(listener).unwrap().await.unwrap();

    task::spawn(async move {
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let stream = acceptor.accept(stream).await.unwrap();
            let (mut reader, mut writer) = smol::io::split(stream);
            match io::copy(&mut reader, &mut writer).await {
                Ok(_) => {}
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::UnexpectedEof {
                        panic!("{}", e);
                    }
                }
            }
        }
    });

    let payload = b"ohai tls";

    let client = tcp.dial(url, None).unwrap().await.unwrap();
    let mut client = tcp.upgrade_dialer(client).unwrap().await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0_u8; 8];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf, payload);
}

#[async_std::test]
#[ignore]
async fn tor_transport_no_control() {
    let url = Url::parse("socks5://127.0.0.1:9050").unwrap();
    let hurl = var("DARKFI_TOR_LOCAL_ADDRESS")
.expect("Please set the env var DARKFI_TOR_LOCAL_ADDRESS to the configured local address in hidden service. \
For example: \'export DARKFI_TOR_LOCAL_ADDRESS=\"tcp://127.0.0.1:8080\"\'");
    let hurl = Url::parse(&hurl).unwrap();

    let onion = var("DARKFI_TOR_PUBLIC_ADDRESS").expect(
        "Please set the env var DARKFI_TOR_PUBLIC_ADDRESS to the configured onion address. \
For example: \'export DARKFI_TOR_PUBLIC_ADDRESS=\"tor://abcdefghij234567.onion\"\'",
    );

    let tor = TorTransport::new(url, None).unwrap();
    let listener = tor.clone().listen_on(hurl).unwrap().await.unwrap();

    task::spawn(async move {
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let (reader, writer) = &mut (&stream, &stream);
            io::copy(reader, writer).await.unwrap();
        }
    });

    let payload = b"ohai tor";
    let url = Url::parse(&onion).unwrap();
    let mut client = tor.dial(url, None).unwrap().await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0_u8; 8];
    client.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, payload);
}

#[async_std::test]
#[ignore]
async fn tor_transport_with_control() {
    let auth_cookie = var("DARKFI_TOR_COOKIE").expect(
        "Please set the env var DARKFI_TOR_COOKIE to the configured tor cookie file. \
For example: \'export DARKFI_TOR_COOKIE=\"/var/lib/tor/control_auth_cookie\"\'",
    );
    let auth_cookie = hex::encode(fs::read(auth_cookie).unwrap());
    let socks_url = Url::parse("socks5://127.0.0.1:9050").unwrap();
    let torc_url = Url::parse("tcp://127.0.0.1:9051").unwrap();
    let local_url = Url::parse("tcp://127.0.0.1:8787").unwrap();

    let tor = TorTransport::new(socks_url, Some((torc_url, auth_cookie))).unwrap();
    // generate EHS pointing to local address
    let hurl = tor.create_ehs(local_url.clone()).unwrap();

    let listener = tor.clone().listen_on(local_url).unwrap().await.unwrap();

    task::spawn(async move {
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let (reader, writer) = &mut (&stream, &stream);
            io::copy(reader, writer).await.unwrap();
        }
    });

    let payload = b"ohai tor";

    let mut client = tor.dial(hurl, None).unwrap().await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0_u8; 8];
    client.read_exact(&mut buf).await.unwrap();
    assert_eq!(buf, payload);
}

#[async_std::test]
#[should_panic(expected = "Socks5Error(ReplyError(HostUnreachable))")]
#[ignore]
async fn tor_transport_with_control_dropped() {
    let auth_cookie = var("DARKFI_TOR_COOKIE").expect(
        "Please set the env var DARKFI_TOR_COOKIE to the configured tor cookie file. \
For example: \'export DARKFI_TOR_COOKIE=\"/var/lib/tor/control_auth_cookie\"\'",
    );
    let auth_cookie = hex::encode(fs::read(auth_cookie).unwrap());
    let socks_url = Url::parse("socks5://127.0.0.1:9050").unwrap();
    let torc_url = Url::parse("tcp://127.0.0.1:9051").unwrap();
    let local_url = Url::parse("tcp://127.0.0.1:8787").unwrap();
    let hurl;
    // We create a new scope for the Transport, to see if when we drop it, the host is still reachable
    {
        let tor = TorTransport::new(socks_url.clone(), Some((torc_url, auth_cookie))).unwrap();
        // generate EHS pointing to local address
        hurl = tor.create_ehs(local_url.clone()).unwrap();
        // Nothing is listening, but the host is reachable.
        // In this case, dialing should fail with Socks5Error(ReplyError(GeneralFailure));
        // And not with Socks5Error(ReplyError(HostUnreachable))
    }

    let tor_client = TorTransport::new(socks_url, None).unwrap();
    // Try to reach the host
    let _client = tor_client.dial(hurl, None).unwrap().await.unwrap();
}

#[async_std::test]
#[ignore]
async fn nym_transport() {
    let target_url = Url::parse("nym://127.0.0.1:25553").unwrap();

    let nym = NymTransport::new().unwrap();

    let listener = nym.clone().listen_on(target_url.clone()).unwrap().await.unwrap();

    task::spawn(async move {
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let (reader, writer) = &mut (&stream, &stream);
            io::copy(reader, writer).await.unwrap();
        }
    });

    let payload = b"ohai nym";

    let mut client = nym.dial(target_url, None).unwrap().await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0_u8; 8];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf, payload);
}

#[async_std::test]
#[ignore]
async fn nym_tls_transport() {
    let target_url = Url::parse("nym+tls://127.0.0.1:25553").unwrap();

    let nym = NymTransport::new().unwrap();

    let listener = nym.clone().listen_on(target_url.clone()).unwrap().await.unwrap();
    let (acceptor, listener) = nym.clone().upgrade_listener(listener).unwrap().await.unwrap();

    task::spawn(async move {
        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let stream = stream.unwrap();
            let stream = acceptor.accept(stream).await.unwrap();
            let (mut reader, mut writer) = smol::io::split(stream);
            io::copy(&mut reader, &mut writer).await.unwrap();
        }
    });

    let payload = b"ohai nymtls";

    let client = nym.clone().dial(target_url, None).unwrap().await.unwrap();
    let mut client = nym.upgrade_dialer(client).unwrap().await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0_u8; 11];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf, payload);
}
