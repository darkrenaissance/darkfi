use std::{net::TcpStream, os::unix::net::UnixStream, str, time::Duration};

use async_std::io::timeout;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
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

impl From<JsonResponse> for JsonResult {
    fn from(resp: JsonResponse) -> Self {
        Self::Resp(resp)
    }
}

impl From<JsonError> for JsonResult {
    fn from(err: JsonError) -> Self {
        Self::Err(err)
    }
}

impl From<JsonNotification> for JsonResult {
    fn from(notif: JsonNotification) -> Self {
        Self::Notif(notif)
    }
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

pub async fn send_request(uri: &Url, data: Value, socks_url: Option<Url>) -> Result<JsonResult> {
    let data_str = serde_json::to_string(&data)?;

    let socket_addr = uri.socket_addrs(|| None)?[0];
    let host = socket_addr.ip().to_string();
    let port = socket_addr.port();

    match uri.scheme() {
        "tcp" | "tls" => {
            let mut stream = Async::<TcpStream>::connect(socket_addr).await?;

            if uri.scheme() == "tls" {
                let mut stream = async_native_tls::connect(&host, stream).await?;
                get_reply(&mut stream, data_str).await
            } else {
                get_reply(&mut stream, data_str).await
            }
        }
        "unix" => {
            let mut stream = Async::<UnixStream>::connect(uri.path()).await?;
            get_reply(&mut stream, data_str).await
        }
        "tor" | "nym" => {
            use fast_socks5::client::{Config, Socks5Stream};

            if socks_url.is_none() {
                return Err(Error::NoSocks5UrlFound)
            }

            let socks_url = socks_url.unwrap();

            let config = Config::default();

            let socks_url_str = socks_url.socket_addrs(|| None)?[0].to_string();

            let mut stream = if !socks_url.username().is_empty() && socks_url.password().is_some() {
                Socks5Stream::connect_with_password(
                    socks_url_str,
                    host,
                    port,
                    socks_url.username().to_string(),
                    socks_url.password().unwrap().to_string(),
                    config,
                )
                .await?
            } else {
                Socks5Stream::connect(socks_url_str, host, port, config).await?
            };

            get_reply(&mut stream, data_str).await
        }
        _ => unimplemented!(),
    }
}

async fn get_reply<T: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut T,
    data_str: String,
) -> Result<JsonResult> {
    // If we don't get a reply after 30 seconds, we'll fail.
    let read_timeout = Duration::from_secs(30);

    let mut buf = [0; 2048];

    stream.write_all(data_str.as_bytes()).await?;

    let bytes_read = timeout(read_timeout, async { stream.read(&mut buf[..]).await }).await?;

    let reply: JsonResult = serde_json::from_slice(&buf[0..bytes_read])?;
    Ok(reply)
}

// Utils to quickly handle errors
pub type ValueResult<Value> = std::result::Result<Value, ErrorCode>;

pub fn from_result(res: ValueResult<Value>, id: Value) -> JsonResult {
    match res {
        Ok(v) => JsonResult::Resp(response(v, id)),
        Err(e) => error(e, None, id).into(),
    }
}
