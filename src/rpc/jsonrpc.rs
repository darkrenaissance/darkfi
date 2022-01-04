use std::{
    net::{TcpStream, ToSocketAddrs},
    os::unix::net::UnixStream,
    str,
};

use async_std::io::{ReadExt, WriteExt};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use smol::Async;

use crate::Error;

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

#[derive(Serialize, Deserialize)]
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

pub async fn send_raw_request(url: &str, data: Value) -> Result<JsonResult, Error> {
    let use_tls: bool;
    let parsed_url = url::Url::parse(url)?;

    match parsed_url.scheme() {
        "tcp" => use_tls = false,
        "tls" => use_tls = true,
        scheme => {
            return Err(Error::UrlParseError(format!(
                "Invalid scheme `{}` found in `{}`",
                scheme, parsed_url
            )))
        }
    }

    let host = parsed_url
        .host()
        .ok_or_else(|| Error::UrlParseError(format!("Missing host in {}", url)))?
        .to_string();
    let port = parsed_url
        .port()
        .ok_or_else(|| Error::UrlParseError(format!("Missing port in {}", url)))?;

    let socket_addr = {
        let host = host.clone();
        smol::unblock(move || (host.as_str(), port).to_socket_addrs())
            .await?
            .next()
            .ok_or(Error::NoUrlFound)?
    };

    let mut buf = [0; 2048];
    let bytes_read: usize;
    let data_str = serde_json::to_string(&data)?;

    let mut stream = Async::<TcpStream>::connect(socket_addr).await?;

    if use_tls {
        let mut stream = async_native_tls::connect(&host, stream).await?;
        stream.write_all(data_str.as_bytes()).await?;
        bytes_read = stream.read(&mut buf[..]).await?;
    } else {
        stream.write_all(data_str.as_bytes()).await?;
        bytes_read = stream.read(&mut buf[..]).await?;
    }

    let reply: JsonResult = serde_json::from_slice(&buf[0..bytes_read])?;
    Ok(reply)
}

pub async fn send_unix_request(path: &str, data: Value) -> Result<JsonResult, Error> {
    let mut buf = [0; 2048];
    let data_str = serde_json::to_string(&data)?;

    let mut stream = Async::<UnixStream>::connect(path).await?;
    stream.write_all(data_str.as_bytes()).await?;

    let bytes_read: usize = stream.read(&mut buf[..]).await?;

    let reply: JsonResult = serde_json::from_slice(&buf[0..bytes_read])?;
    Ok(reply)
}
