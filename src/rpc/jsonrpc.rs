use std::{
    net::{TcpStream, ToSocketAddrs},
    os::unix::net::UnixStream,
    str,
    time::Duration,
};

use async_std::io::{timeout, ReadExt, WriteExt};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use smol::Async;
use url::Url;

use crate::{Error, Result};

#[derive(Debug, Clone)]
pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    KeyGenError,
    GetAddressesError,
    ImportAndExportFile,
    SetDefaultAddress,
    InvalidAmountParam,
    InvalidNetworkParam,
    InvalidTokenIdParam,
    InvalidAddressParam,
    InvalidSymbolParam,
    ServerError(i64),
}

impl ErrorCode {
    pub fn code(&self) -> i64 {
        match *self {
            ErrorCode::ParseError => -32700,
            ErrorCode::InvalidRequest => -32600,
            ErrorCode::MethodNotFound => -32601,
            ErrorCode::InvalidParams => -32602,
            ErrorCode::InternalError => -32603,
            ErrorCode::KeyGenError => -32002,
            ErrorCode::GetAddressesError => -32003,
            ErrorCode::ImportAndExportFile => -32004,
            ErrorCode::SetDefaultAddress => -32005,
            ErrorCode::InvalidAmountParam => -32010,
            ErrorCode::InvalidNetworkParam => -32011,
            ErrorCode::InvalidTokenIdParam => -32012,
            ErrorCode::InvalidAddressParam => -32013,
            ErrorCode::InvalidSymbolParam => -32014,
            ErrorCode::ServerError(c) => c,
        }
    }

    pub fn description(&self) -> String {
        let desc = match *self {
            ErrorCode::ParseError => "Parse error",
            ErrorCode::InvalidRequest => "Invalid request",
            ErrorCode::MethodNotFound => "Method not found",
            ErrorCode::InvalidParams => "Invalid params",
            ErrorCode::InternalError => "Internal error",
            ErrorCode::KeyGenError => "Key gen error",
            ErrorCode::GetAddressesError => "get addresses error",
            ErrorCode::ImportAndExportFile => "error import/export a file",
            ErrorCode::SetDefaultAddress => "error set default address",
            ErrorCode::InvalidAmountParam => "Invalid amount param",
            ErrorCode::InvalidNetworkParam => "Invalid network param",
            ErrorCode::InvalidTokenIdParam => "Invalid token id param",
            ErrorCode::InvalidAddressParam => "Invalid address param",
            ErrorCode::InvalidSymbolParam => "Invalid symbol param",
            ErrorCode::ServerError(_) => "Server error",
        };
        desc.to_string()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum JsonResult {
    Resp(JsonResponse),
    Err(JsonError),
    Notif(JsonNotification),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JsonRequest {
    pub jsonrpc: Value,
    pub method: Value,
    pub params: Value,
    pub id: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JsonErrorVal {
    pub code: Value,
    pub message: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JsonError {
    pub jsonrpc: Value,
    pub error: JsonErrorVal,
    pub id: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JsonResponse {
    pub jsonrpc: Value,
    pub result: Value,
    pub id: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JsonNotification {
    pub jsonrpc: Value,
    pub method: Value,
    pub params: Value,
}

pub fn request(m: Value, p: Value) -> JsonRequest {
    let mut rng = rand::thread_rng();

    JsonRequest { jsonrpc: json!("2.0"), method: m, params: p, id: json!(rng.gen::<u32>()) }
}

pub fn response(r: Value, i: Value) -> JsonResponse {
    JsonResponse { jsonrpc: json!("2.0"), result: r, id: i }
}

pub fn error(c: ErrorCode, m: Option<String>, i: Value) -> JsonError {
    let ev = JsonErrorVal {
        code: json!(c.code()),
        message: if m.is_none() { json!(c.description()) } else { json!(Some(m)) },
    };

    JsonError { jsonrpc: json!("2.0"), error: ev, id: i }
}

pub fn notification(m: Value, p: Value) -> JsonNotification {
    JsonNotification { jsonrpc: json!("2.0"), method: m, params: p }
}

pub async fn send_request(uri: &Url, data: Value, _socks_url: Option<Url>) -> Result<JsonResult> {
    // If we don't get a reply after 30 seconds, we'll fail.
    let read_timeout = Duration::from_secs(30);

    let mut buf = [0; 2048];
    let bytes_read: usize;
    let data_str = serde_json::to_string(&data)?;

    match uri.scheme() {
        "tcp" | "tls" => {
            let host = uri
                .host()
                .ok_or_else(|| Error::UrlParseError(format!("Missing host in {}", uri)))?
                .to_string();

            let port = uri
                .port()
                .ok_or_else(|| Error::UrlParseError(format!("Missing port in {}", uri)))?;

            let socket_addr = {
                let host = host.clone();
                smol::unblock(move || (host.as_str(), port).to_socket_addrs())
                    .await?
                    .next()
                    .ok_or(Error::NoUrlFound)?
            };

            let mut stream = Async::<TcpStream>::connect(socket_addr).await?;

            if uri.scheme() == "tls" {
                let mut stream = async_native_tls::connect(&host, stream).await?;
                stream.write_all(data_str.as_bytes()).await?;
                bytes_read =
                    timeout(read_timeout, async { stream.read(&mut buf[..]).await }).await?;
            } else {
                stream.write_all(data_str.as_bytes()).await?;
                bytes_read =
                    timeout(read_timeout, async { stream.read(&mut buf[..]).await }).await?;
            }

            let reply: JsonResult = serde_json::from_slice(&buf[0..bytes_read])?;
            Ok(reply)
        }

        "unix" => {
            let mut stream = Async::<UnixStream>::connect(uri.path()).await?;
            stream.write_all(data_str.as_bytes()).await?;

            bytes_read = timeout(read_timeout, async { stream.read(&mut buf[..]).await }).await?;

            let reply: JsonResult = serde_json::from_slice(&buf[0..bytes_read])?;
            Ok(reply)
        }

        "tor" => unimplemented!(),
        "nym" => unimplemented!(),
        _ => unreachable!(),
    }
}
