/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use darkfi_serial::{AsyncDecodable, AsyncEncodable};
use smol::{io, LocalExecutor};
use url::Url;

use darkfi::net::transport::{Dialer, Listener};

#[test]
fn tcp_transport() {
    let executor = LocalExecutor::new();
    let url = Url::parse("tcp://127.0.0.1:5432").unwrap();

    smol::block_on(executor.run(async {
        let listener = Listener::new(url.clone()).await.unwrap().listen().await.unwrap();
        executor
            .spawn(async move {
                let (stream, _) = listener.next().await.unwrap();
                let (mut reader, mut writer) = smol::io::split(stream);
                io::copy(&mut reader, &mut writer).await.unwrap();
            })
            .detach();

        let payload = "ohai tcp";

        let dialer = Dialer::new(url).await.unwrap();
        let mut client = dialer.dial(None).await.unwrap();
        payload.encode_async(&mut client).await.unwrap();

        let buf: String = AsyncDecodable::decode_async(&mut client).await.unwrap();

        assert_eq!(buf, payload);
    }));
}

#[test]
fn tcp_tls_transport() {
    let executor = LocalExecutor::new();
    let url = Url::parse("tcp+tls://127.0.0.1:5433").unwrap();

    smol::block_on(executor.run(async {
        let listener = Listener::new(url.clone()).await.unwrap().listen().await.unwrap();
        executor
            .spawn(async move {
                let (stream, _) = listener.next().await.unwrap();
                let (mut reader, mut writer) = smol::io::split(stream);
                io::copy(&mut reader, &mut writer).await.unwrap();
            })
            .detach();

        let payload = "ohai tls";

        let dialer = Dialer::new(url).await.unwrap();
        let mut client = dialer.dial(None).await.unwrap();
        payload.encode_async(&mut client).await.unwrap();

        let buf: String = AsyncDecodable::decode_async(&mut client).await.unwrap();

        assert_eq!(buf, payload);
    }));
}

#[test]
fn unix_transport() {
    let executor = LocalExecutor::new();

    let tmpdir = std::env::temp_dir();
    let url = Url::parse(&format!(
        "unix://{}/darkfi_unix_plain.sock",
        tmpdir.as_os_str().to_str().unwrap()
    ))
    .unwrap();

    smol::block_on(executor.run(async {
        let listener = Listener::new(url.clone()).await.unwrap().listen().await.unwrap();
        executor
            .spawn(async move {
                let (stream, _) = listener.next().await.unwrap();
                let (mut reader, mut writer) = smol::io::split(stream);
                io::copy(&mut reader, &mut writer).await.unwrap();
            })
            .detach();

        let payload = "ohai unix";

        let dialer = Dialer::new(url).await.unwrap();
        let mut client = dialer.dial(None).await.unwrap();
        payload.encode_async(&mut client).await.unwrap();

        let buf: String = AsyncDecodable::decode_async(&mut client).await.unwrap();

        assert_eq!(buf, payload);
    }));
}
