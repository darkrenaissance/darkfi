/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use futures::AsyncRead;
use smol::io::{self, AsyncReadExt};

pub fn hash_to_string(hash: &blake3::Hash) -> String {
    bs58::encode(hash.as_bytes()).into_string()
}

/// smol::fs::File::read does not guarantee that the buffer will be filled, even if the buffer is
/// smaller than the file. This is a workaround.
/// This reads the stream until the buffer is full or until we reached the end of the stream.
pub async fn read_until_filled(
    mut stream: impl AsyncRead + Unpin,
    buffer: &mut [u8],
) -> io::Result<&[u8]> {
    let mut total_bytes_read = 0;

    while total_bytes_read < buffer.len() {
        let bytes_read = stream.read(&mut buffer[total_bytes_read..]).await?;
        if bytes_read == 0 {
            break; // EOF reached
        }
        total_bytes_read += bytes_read;
    }

    Ok(&buffer[..total_bytes_read])
}
