use async_std::sync::Arc;
use std::{env, str, time::Duration};

use async_executor::Executor;
use async_std::io::timeout;
use futures::{AsyncReadExt, AsyncWriteExt};
use log::error;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

use crate::{
    net::{TcpTransport, TorTransport, Transport, TransportName, TransportStream, UnixTransport},
    Error, Result,
};

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

async fn reqrep_loop<T: TransportStream>(
    mut stream: T,
    data_receiver: async_channel::Receiver<Value>,
    result_sender: async_channel::Sender<JsonResult>,
) -> Result<()> {
    // If we don't get a reply after 30 seconds, we'll fail.
    let read_timeout = Duration::from_secs(30);

    loop {
        let mut buf = [0; 8192];

        let data = data_receiver.recv().await?;
        let data_str = serde_json::to_string(&data)?;

        stream.write_all(data_str.as_bytes()).await?;

        let bytes_read = timeout(read_timeout, async { stream.read(&mut buf[..]).await }).await?;

        let reply: JsonResult = serde_json::from_slice(&buf[0..bytes_read])?;

        result_sender.send(reply).await?;
    }
}

pub async fn open_channels(
    uri: &Url,
    executor: Arc<Executor<'_>>,
) -> Result<(async_channel::Sender<Value>, async_channel::Receiver<JsonResult>)> {
    let (data_sender, data_receiver) = async_channel::unbounded();
    let (result_sender, result_receiver) = async_channel::unbounded();

    let transport_name = TransportName::try_from(uri.clone())?;

    macro_rules! hanlde_stream {
        ($stream:expr, $transport:expr, $upgrade:expr) => {
            if let Err(err) = $stream {
                error!("RPC Setup for {} failed: {}", uri, err);
                return Err(Error::ConnectFailed)
            }

            let stream = $stream?.await;

            if let Err(err) = stream {
                error!("RPC Connection to {} failed: {}", uri, err);
                return Err(Error::ConnectFailed)
            }

            let stream = stream?;

            match $upgrade {
                None => {
                    executor.spawn(reqrep_loop(stream, data_receiver, result_sender)).detach();
                }
                Some(u) if u == "tls" => {
                    let stream = $transport.upgrade_dialer(stream)?.await?;
                    executor.spawn(reqrep_loop(stream, data_receiver, result_sender)).detach();
                }
                Some(u) => return Err(Error::UnsupportedTransportUpgrade(u)),
            }
        };
    }

    match transport_name {
        TransportName::Tcp(upgrade) => {
            let transport = TcpTransport::new(None, 1024);
            let stream = transport.dial(uri.clone());

            hanlde_stream!(stream, transport, upgrade);
        }
        TransportName::Tor(upgrade) => {
            let socks5_url = Url::parse(
                &env::var("DARKFI_TOR_SOCKS5_URL")
                    .unwrap_or_else(|_| "socks5://127.0.0.1:9050".to_string()),
            )?;

            let transport = TorTransport::new(socks5_url, None)?;

            let stream = transport.clone().dial(uri.clone());

            hanlde_stream!(stream, transport, upgrade);
        }
        TransportName::Unix => {
            let transport = UnixTransport::new();

            let stream = transport.dial(uri.clone()).await;

            if let Err(err) = stream {
                error!("RPC Connection to {}  failed: {}", uri, err);
                return Err(Error::ConnectFailed)
            }

            executor.spawn(reqrep_loop(stream?, data_receiver, result_sender)).detach();
        }
        _ => unimplemented!(),
    }

    Ok((data_sender, result_receiver))
}

pub async fn send_request(uri: &Url, data: Value) -> Result<JsonResult> {
    let data_str = serde_json::to_string(&data)?;

    let transport_name = TransportName::try_from(uri.clone())?;

    match transport_name {
        TransportName::Tcp(upgrade) => {
            let transport = TcpTransport::new(None, 1024);
            let stream = transport.dial(uri.clone());

            if let Err(err) = stream {
                error!("RPC Setup for {} failed: {}", uri, err);
                return Err(Error::ConnectFailed)
            }

            let stream = stream?.await;

            if let Err(err) = stream {
                error!("RPC Connection to {}  failed: {}", uri, err);
                return Err(Error::ConnectFailed)
            }

            match upgrade {
                None => get_reply(&mut stream?, data_str).await,
                Some(u) if u == "tls" => {
                    let mut stream = transport.upgrade_dialer(stream?)?.await?;
                    get_reply(&mut stream, data_str).await
                }
                Some(u) => Err(Error::UnsupportedTransportUpgrade(u)),
            }
        }
        TransportName::Tor(upgrade) => {
            let socks5_url = Url::parse(
                &env::var("DARKFI_TOR_SOCKS5_URL")
                    .unwrap_or_else(|_| "socks5://127.0.0.1:9050".to_string()),
            )?;

            let transport = TorTransport::new(socks5_url, None)?;

            let stream = transport.clone().dial(uri.clone());

            if let Err(err) = stream {
                error!("RPC Setup for {} failed: {}", uri, err);
                return Err(Error::ConnectFailed)
            }

            let stream = stream?.await;

            if let Err(err) = stream {
                error!("RPC Connection to {} failed: {}", uri, err);
                return Err(Error::ConnectFailed)
            }

            match upgrade {
                None => get_reply(&mut stream?, data_str).await,
                Some(u) if u == "tls" => {
                    let mut stream = transport.upgrade_dialer(stream?)?.await?;
                    get_reply(&mut stream, data_str).await
                }
                Some(u) => Err(Error::UnsupportedTransportUpgrade(u)),
            }
        }
        TransportName::Unix => {
            let transport = UnixTransport::new();

            let stream = transport.dial(uri.clone()).await;

            if let Err(err) = stream {
                error!("RPC Connection to {}  failed: {}", uri, err);
                return Err(Error::ConnectFailed)
            }

            get_reply(&mut stream?, data_str).await
        }
        _ => unimplemented!(),
    }
}

async fn get_reply<T: TransportStream>(stream: &mut T, data_str: String) -> Result<JsonResult> {
    // If we don't get a reply after 30 seconds, we'll fail.
    let read_timeout = Duration::from_secs(30);

    let mut buf = [0; 8192];

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
