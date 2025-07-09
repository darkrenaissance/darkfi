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

use std::{io, time::Duration};

use smol::io::{AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tracing::error;

use super::jsonrpc::*;
use crate::net::transport::PtStream;

pub(super) const INIT_BUF_SIZE: usize = 4096; // 4K
pub(super) const MAX_BUF_SIZE: usize = 1024 * 1024 * 16; // 16M
pub(super) const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Internal read function that reads from the active stream into a buffer.
/// Performs HTTP POST request parsing. Returns the request body length.
pub(super) async fn http_read_from_stream_request(
    reader: &mut BufReader<ReadHalf<Box<dyn PtStream>>>,
    buf: &mut Vec<u8>,
) -> io::Result<usize> {
    let mut total_read = 0;

    // Intermediate buffer we use to read byte-by-byte.
    let mut tmpbuf = [0_u8];

    while total_read < MAX_BUF_SIZE {
        buf.resize(total_read + INIT_BUF_SIZE, 0u8);

        match reader.read(&mut tmpbuf).await {
            Ok(0) if total_read == 0 => return Err(io::ErrorKind::ConnectionAborted.into()),
            Ok(0) => break, // Finished reading
            Ok(_) => {
                // Copy the read byte to the destination buffer.
                buf[total_read] = tmpbuf[0];
                total_read += 1;

                // In HTTP, when we reach '\r\n\r\n' we know we've read the headers.
                // The rest is the body. Headers should contain Content-Length which
                // tells us the remaining amount of bytes to read.
                if total_read > 4 && buf[total_read - 4..total_read] == [b'\r', b'\n', b'\r', b'\n']
                {
                    break
                }
            }

            Err(e) => return Err(e),
        }
    }

    // Here we parse the HTTP for correctness and find Content-Length
    let mut headers = [httparse::EMPTY_HEADER; 8];
    let mut req = httparse::Request::new(&mut headers);
    let _body_offset = match req.parse(buf) {
        Ok(v) => v.unwrap(), // TODO: This should check httparse::Status::is_partial()
        Err(e) => {
            error!("[RPC] Failed parsing HTTP request: {e}");
            return Err(io::ErrorKind::InvalidData.into())
        }
    };

    let mut content_length: usize = 0;
    for header in headers {
        if header.name.to_lowercase() == "content-length" {
            let s = String::from_utf8_lossy(header.value);
            content_length = match s.parse() {
                Ok(v) => v,
                Err(_) => return Err(io::ErrorKind::InvalidData.into()),
            };
        }
    }

    if content_length == 0 || content_length > MAX_BUF_SIZE {
        return Err(io::ErrorKind::InvalidData.into())
    }

    // Now we know the request body size. Read it into the buffer.
    buf.clear();
    buf.resize(content_length, 0_u8);
    reader.read(buf).await?;

    assert!(buf.len() == content_length);
    Ok(content_length)
}

/// Internal read function that reads from the active stream into a buffer.
/// Performs HTTP POST response parsing. Returns the response body length.
pub(super) async fn http_read_from_stream_response(
    reader: &mut BufReader<ReadHalf<Box<dyn PtStream>>>,
    buf: &mut Vec<u8>,
) -> io::Result<usize> {
    let mut total_read = 0;

    // Intermediate buffer we use to read byte-by-byte.
    let mut tmpbuf = [0_u8];

    while total_read < MAX_BUF_SIZE {
        buf.resize(total_read + INIT_BUF_SIZE, 0u8);

        match reader.read(&mut tmpbuf).await {
            Ok(0) if total_read == 0 => return Err(io::ErrorKind::ConnectionAborted.into()),
            Ok(0) => break, // Finished reading
            Ok(_) => {
                // Copy the read byte to the destination buffer.
                buf[total_read] = tmpbuf[0];
                total_read += 1;

                // In HTTP, when we reach '\r\n\r\n' we know we've read the headers.
                // The rest is the body. Headers should contain Content-Length which
                // tells us the remaining amount of bytes to read.
                if total_read > 4 && buf[total_read - 4..total_read] == [b'\r', b'\n', b'\r', b'\n']
                {
                    break
                }
            }

            Err(e) => return Err(e),
        }
    }

    // Here we parse the HTTP for correctness and find Content-Length
    let mut headers = [httparse::EMPTY_HEADER; 8];
    let mut resp = httparse::Response::new(&mut headers);
    let _body_offset = match resp.parse(buf) {
        Ok(v) => v.unwrap(), // TODO: This should check httparse::Status::is_partial()
        Err(e) => {
            error!("[RPC] Failed parsing HTTP response: {e}");
            return Err(io::ErrorKind::InvalidData.into())
        }
    };

    let mut content_length: usize = 0;
    for header in headers {
        if header.name.to_lowercase() == "content-length" {
            let s = String::from_utf8_lossy(header.value);
            content_length = match s.parse() {
                Ok(v) => v,
                Err(_) => return Err(io::ErrorKind::InvalidData.into()),
            };
        }
    }

    if content_length == 0 || content_length > MAX_BUF_SIZE {
        return Err(io::ErrorKind::InvalidData.into())
    }

    // Now we know the response body size. Read it into the buffer.
    buf.clear();
    buf.resize(content_length, 0_u8);
    reader.read(buf).await?;

    assert!(buf.len() == content_length);
    Ok(content_length)
}

/// Internal read function that reads from the active stream into a buffer.
/// Reading stops upon reaching CRLF or LF, or when `MAX_BUF_SIZE` is reached.
pub(super) async fn read_from_stream(
    reader: &mut BufReader<ReadHalf<Box<dyn PtStream>>>,
    buf: &mut Vec<u8>,
) -> io::Result<usize> {
    let mut total_read = 0;

    // Intermediate buffer we use to read byte-by-byte.
    let mut tmpbuf = [0_u8];

    while total_read < MAX_BUF_SIZE {
        buf.resize(total_read + INIT_BUF_SIZE, 0u8);

        match reader.read(&mut tmpbuf).await {
            Ok(0) if total_read == 0 => return Err(io::ErrorKind::ConnectionAborted.into()),
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

            Err(e) => return Err(e),
        }
    }

    // Truncate buffer to actual data size
    buf.truncate(total_read);
    Ok(total_read)
}

/// Internal write function that writes a JSON-RPC object to the active stream.
/// Sent as an HTTP response.
pub(super) async fn http_write_to_stream(
    writer: &mut WriteHalf<Box<dyn PtStream>>,
    object: &JsonResult,
) -> io::Result<()> {
    let (status_line, object_str) = match object {
        JsonResult::Notification(v) => ("HTTP/1.1 200 OK", v.stringify().unwrap()),
        JsonResult::Response(v) => ("HTTP/1.1 200 OK", v.stringify().unwrap()),
        JsonResult::Error(v) => ("HTTP/1.1 400 Bad Request", v.stringify().unwrap()),
        JsonResult::Request(v) => ("POST /json_rpc HTTP/1.1", v.stringify().unwrap()),
        _ => unreachable!(),
    };

    let length = object_str.len();
    let data = format!("{status_line}\r\nContent-Length: {length}\r\nContent-Type: application/json\r\n\r\n{object_str}");

    writer.write_all(data.as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}

/// Internal write function that writes a JSON-RPC object to the active stream.
pub(super) async fn write_to_stream(
    writer: &mut WriteHalf<Box<dyn PtStream>>,
    object: &JsonResult,
) -> io::Result<()> {
    let object_str = match object {
        JsonResult::Notification(v) => v.stringify().unwrap(),
        JsonResult::Response(v) => v.stringify().unwrap(),
        JsonResult::Error(v) => v.stringify().unwrap(),
        JsonResult::Request(v) => v.stringify().unwrap(),
        _ => unreachable!(),
    };

    // As we're a line-based protocol, we append CRLF to the end of the JSON string.
    for i in [object_str.as_bytes(), b"\r\n"] {
        writer.write_all(i).await?
    }

    writer.flush().await?;

    Ok(())
}
