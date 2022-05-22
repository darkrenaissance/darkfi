//! JSON-RPC 2.0 primitives
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// JSON-RPC error codes.
/// The error codes from and including -32768 to -32000 are reserved for pre-defined errors.
#[derive(Debug, Clone)]
pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    ServerError(i64),
    InvalidId,
}

impl ErrorCode {
    pub fn code(&self) -> i64 {
        match *self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
            // -32000 to -32099
            Self::ServerError(c) => c,
            Self::InvalidId => -32001,
        }
    }

    pub fn desc(&self) -> String {
        let desc = match *self {
            Self::ParseError => "Parse error",
            Self::InvalidRequest => "Invalid request",
            Self::MethodNotFound => "Method not found",
            Self::InvalidParams => "Invalid params",
            Self::InternalError => "Internal error",
            Self::ServerError(_) => "",
            Self::InvalidId => "Request ID mismatch",
        };

        desc.to_string()
    }
}

/// Wrapping enum around the possible JSON-RPC object types.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonResult {
    Response(JsonResponse),
    Error(JsonError),
    Notification(JsonNotification),
}

impl From<JsonResponse> for JsonResult {
    fn from(resp: JsonResponse) -> Self {
        Self::Response(resp)
    }
}

impl From<JsonError> for JsonResult {
    fn from(err: JsonError) -> Self {
        Self::Error(err)
    }
}

impl From<JsonNotification> for JsonResult {
    fn from(notif: JsonNotification) -> Self {
        Self::Notification(notif)
    }
}

/// A JSON-RPC request object.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonRequest {
    /// JSON-RPC version
    pub jsonrpc: Value,
    /// Request ID
    pub id: Value,
    /// Request method
    pub method: Value,
    /// Request parameters
    pub params: Value,
}

impl JsonRequest {
    pub fn new(method: &str, parameters: Value) -> Self {
        let mut rng = rand::thread_rng();

        Self {
            jsonrpc: json!("2.0"),
            id: json!(rng.gen::<u64>()),
            method: json!(method),
            params: parameters,
        }
    }
}

/// A JSON-RPC notification object.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonNotification {
    /// JSON-RPC version
    pub jsonrpc: Value,
    /// Notification method
    pub method: Value,
    /// Notification parameters
    pub params: Value,
}

impl JsonNotification {
    pub fn new(method: &str, parameters: Value) -> Self {
        Self { jsonrpc: json!("2.0"), method: json!(method), params: parameters }
    }
}

/// A JSON-RPC response object.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonResponse {
    /// JSON-RPC version
    pub jsonrpc: Value,
    /// Request ID
    pub id: Value,
    /// Response result
    pub result: Value,
}

impl JsonResponse {
    pub fn new(result: Value, id: Value) -> Self {
        Self { jsonrpc: json!("2.0"), id, result }
    }
}

/// A JSON-RPC error object.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonError {
    /// JSON-RPC version
    pub jsonrpc: Value,
    /// Request ID
    pub id: Value,
    /// JSON-RPC error (code and message)
    pub error: JsonErrorVal,
}

/// A JSON-RPC error value (code and message)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JsonErrorVal {
    /// Error code
    pub code: Value,
    /// Error message
    pub message: Value,
}

impl JsonError {
    pub fn new(c: ErrorCode, m: Option<String>, id: Value) -> Self {
        let error = JsonErrorVal {
            code: json!(c.code()),
            message: if m.is_none() { json!(c.desc()) } else { json!(m.unwrap()) },
        };

        Self { jsonrpc: json!("2.0"), error, id }
    }
}
