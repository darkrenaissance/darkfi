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

use std::time::Duration;

use smol::io::{AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};

use super::jsonrpc::*;
use crate::{error::RpcError, net::transport::PtStream, system::io_timeout, Result};

pub(super) const INIT_BUF_SIZE: usize = 4096; // 4K
pub(super) const MAX_BUF_SIZE: usize = 1024 * 8192; // 8M
pub(super) const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Internal read function that reads from the active stream into a buffer.
/// Reading stops upon reaching CRLF or LF, or when `MAX_BUF_SIZE` is reached.
pub(super) async fn read_from_stream(
    reader: &mut BufReader<ReadHalf<Box<dyn PtStream>>>,
    buf: &mut Vec<u8>,
    with_timeout: bool,
) -> Result<usize> {
    let mut total_read = 0;

    // Intermediate buffer we use to read byte-by-byte.
    let mut tmpbuf = [0_u8];

    while total_read < MAX_BUF_SIZE {
        buf.resize(total_read + INIT_BUF_SIZE, 0);

        // Lame we have to duplicate this code, but it is what it is.
        if with_timeout {
            match io_timeout(READ_TIMEOUT, reader.read(&mut tmpbuf)).await {
                Ok(0) if total_read == 0 => {
                    return Err(
                        RpcError::ConnectionClosed("Connection closed cleanly".to_string()).into()
                    )
                }
                Ok(0) => break, // Finished reading
                Ok(_) => {
                    // When we reach '\n', pop a possible '\r' from the buffer and bail.
                    if tmpbuf[0] == b'\n' {
                        if buf[total_read - 1] == b'\r' {
                            buf.pop();
                            total_read -= 1;
                        }
                        break
                    }

                    // Copy the read byte to the destination buffer.
                    buf[total_read] = tmpbuf[0];
                    total_read += 1;
                }

                Err(e) => return Err(RpcError::IoError(e.kind()).into()),
            }
        } else {
            match reader.read(&mut tmpbuf).await {
                Ok(0) if total_read == 0 => {
                    return Err(
                        RpcError::ConnectionClosed("Connection closed cleanly".to_string()).into()
                    )
                }
                Ok(0) => break, // Finished reading
                Ok(_) => {
                    // When we reach '\n', pop a possible '\r' from the buffer and bail.
                    if tmpbuf[0] == b'\n' {
                        if buf[total_read - 1] == b'\r' {
                            buf.pop();
                            total_read -= 1;
                        }
                        break
                    }

                    // Copy the read byte to the destination buffer.
                    buf[total_read] = tmpbuf[0];
                    total_read += 1;
                }

                Err(e) => return Err(RpcError::IoError(e.kind()).into()),
            }
        }
    }

    // Trunacate buffer to actual data size
    buf.truncate(total_read);
    Ok(total_read)
}

/// Internal write function that writes a JSON-RPC object to the active stream.
pub(super) async fn write_to_stream(
    writer: &mut WriteHalf<Box<dyn PtStream>>,
    object: &JsonResult,
) -> Result<()> {
    let object_str = match object {
        JsonResult::Notification(v) => v.stringify()?,
        JsonResult::Response(v) => v.stringify()?,
        JsonResult::Error(v) => v.stringify()?,
        JsonResult::Request(v) => v.stringify()?,
        _ => unreachable!(),
    };

    // As we're a line-based protocol, we append CRLF to the end of the JSON string.
    for i in [object_str.as_bytes(), &[b'\r', b'\n']] {
        if let Err(e) = writer.write_all(i).await {
            return Err(e.into())
        }
    }

    Ok(())
}
