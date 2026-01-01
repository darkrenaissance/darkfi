/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi::net::transport::socks5::Socks5Client;
use futures::{AsyncReadExt, AsyncWriteExt};

#[test]
#[ignore]
fn socks_test() {
    smol::block_on(async {
        let client = Socks5Client::new("127.0.0.1", 9050);
        //let client = Socks5Client::new("127.0.0.1", 1080);
        let mut stream = client.connect(("icanhazip.com", 80)).await.unwrap();

        let req = b"GET / HTTP/1.1\r\nHost: icanhazip.com\r\nConnection: close\r\n\r\n";
        stream.write_all(req).await.unwrap();
        stream.flush().await.unwrap();

        let mut buf = vec![0u8; 1024];
        stream.read_to_end(&mut buf).await.unwrap();

        println!("{}", String::from_utf8(buf).unwrap());
    });
}
