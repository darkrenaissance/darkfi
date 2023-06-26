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

use async_std::{
    io,
    io::{ReadExt, WriteExt},
    task,
};
use url::Url;

use darkfi::net::transport::{Dialer, Listener};

#[async_std::test]
async fn tcp_transport() {
    let url = Url::parse("tcp://127.0.0.1:5432").unwrap();
    let listener = Listener::new(url.clone()).await.unwrap().listen().await.unwrap();
    task::spawn(async move {
        let (stream, _) = listener.next().await.unwrap();
        let (mut reader, mut writer) = smol::io::split(stream);
        io::copy(&mut reader, &mut writer).await.unwrap();
    });

    let payload = b"ohai tcp";

    let dialer = Dialer::new(url).await.unwrap();
    let mut client = dialer.dial(None).await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0u8; 8];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf, payload);
}

#[async_std::test]
async fn tcp_tls_transport() {
    let url = Url::parse("tcp+tls://127.0.0.1:5433").unwrap();
    let listener = Listener::new(url.clone()).await.unwrap().listen().await.unwrap();
    task::spawn(async move {
        let (stream, _) = listener.next().await.unwrap();
        let (mut reader, mut writer) = smol::io::split(stream);
        io::copy(&mut reader, &mut writer).await.unwrap();
    });

    let payload = b"ohai tls";

    let dialer = Dialer::new(url).await.unwrap();
    let mut client = dialer.dial(None).await.unwrap();
    client.write_all(payload).await.unwrap();
    let mut buf = vec![0u8; 8];
    client.read_exact(&mut buf).await.unwrap();

    assert_eq!(buf, payload);
}
