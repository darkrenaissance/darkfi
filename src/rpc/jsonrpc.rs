use std::str;

use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::Error;

#[derive(Debug, Clone)]
pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
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
            ErrorCode::ServerError(_) => "Server error",
        };
        desc.to_string()
    }
}

#[serde(untagged)]
#[derive(Serialize, Deserialize, Debug)]
pub enum JsonResult {
    Resp(JsonResponse),
    Err(JsonError),
    Notif(JsonNotification),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonRequest {
    pub jsonrpc: Value,
    pub method: Value,
    pub params: Value,
    pub id: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonErrorVal {
    pub code: Value,
    pub message: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonError {
    pub jsonrpc: Value,
    pub error: JsonErrorVal,
    pub id: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonResponse {
    pub jsonrpc: Value,
    pub result: Value,
    pub id: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonNotification {
    pub jsonrpc: Value,
    pub method: Value,
    pub params: Value,
}

pub fn request(m: Value, p: Value) -> JsonRequest {
    let mut rng = rand::thread_rng();

    JsonRequest {
        jsonrpc: json!("2.0"),
        method: m,
        params: p,
        id: json!(rng.gen::<u32>()),
    }
}

pub fn response(r: Value, i: Value) -> JsonResponse {
    JsonResponse {
        jsonrpc: json!("2.0"),
        result: r,
        id: i,
    }
}

pub fn error(c: ErrorCode, m: Option<String>, i: Value) -> JsonError {
    let ev = JsonErrorVal {
        code: json!(c.code()),
        message: if m.is_none() {
            json!(c.description())
        } else {
            json!(Some(m))
        },
    };

    JsonError {
        jsonrpc: json!("2.0"),
        error: ev,
        id: i,
    }
}

pub fn notification(m: Value, p: Value) -> JsonNotification {
    JsonNotification {
        jsonrpc: json!("2.0"),
        method: m,
        params: p,
    }
}

pub async fn send_request(url: &str, data: Value) -> Result<JsonResult, Error> {
    // TODO: TLS
    let mut buf = [0; 2048];
    let mut stream = TcpStream::connect(url).await?;
    let data_str = serde_json::to_string(&data)?;

    stream.write_all(&data_str.as_bytes()).await?;
    let bytes_read = stream.read(&mut buf[..]).await?;

    let reply: JsonResult = serde_json::from_slice(&buf[0..bytes_read])?;
    Ok(reply)
}
