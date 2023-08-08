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

//! JSON-RPC 2.0 primitives
use std::fmt;

use async_std::sync::Arc;
use darkfi_serial::{serialize, Encodable};
use rand::{rngs::OsRng, Rng};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Value};

use crate::system::{Subscriber, SubscriberPtr};

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
// ANCHOR: jsonresult
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonResult {
    Response(JsonResponse),
    Error(JsonError),
    Notification(JsonNotification),
    Subscriber(JsonSubscriber),
}
// ANCHOR_END: jsonresult

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

impl From<JsonSubscriber> for JsonResult {
    fn from(sub: JsonSubscriber) -> Self {
        Self::Subscriber(sub)
    }
}

/// A JSON-RPC request object.
// ANCHOR: jsonrequest
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
// ANCHOR_END: jsonrequest

impl JsonRequest {
    pub fn new(method: &str, parameters: Value) -> Self {
        Self {
            jsonrpc: json!("2.0"),
            id: json!(OsRng.gen::<u64>()),
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
    pub fn new(method: Value, params: Value) -> Self {
        Self { jsonrpc: json!("2.0"), method, params }
    }
}

/// A method specific JSON-RPC subscriber for notifications
#[derive(Clone)]
pub struct MethodSubscriber {
    /// Notification method
    pub method: Value,
    /// Notification subscriber
    pub subscriber: SubscriberPtr<JsonNotification>,
}

impl MethodSubscriber {
    pub fn new(method: Value) -> Self {
        let subscriber = Subscriber::new();
        Self { method, subscriber }
    }

    /// Auxiliary function to format provided message and notify the subscriber.
    pub async fn notify<T: Encodable>(&self, message: &T) {
        let params = json!([bs58::encode(&serialize(message)).into_string()]);
        let notif = JsonNotification::new(self.method.clone(), params);
        self.subscriber.notify(notif).await;
    }
}

impl fmt::Debug for MethodSubscriber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MethodSubscriber")
            .field("method", &self.method)
            .field("pointer", &Arc::as_ptr(&self.subscriber))
            .finish()
    }
}

/// A JSON-RPC subscriber for notifications
#[derive(Clone, Debug)]
pub struct JsonSubscriber {
    /// JSON-RPC version
    pub jsonrpc: Value,
    /// Method subscriber
    pub subscriber: MethodSubscriber,
}

impl JsonSubscriber {
    pub fn new(subscriber: MethodSubscriber) -> Self {
        Self { jsonrpc: json!("2.0"), subscriber }
    }
}

impl Serialize for JsonSubscriber {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        unimplemented!();
    }
}

impl<'de> Deserialize<'de> for JsonSubscriber {
    fn deserialize<D>(_deserializer: D) -> Result<JsonSubscriber, D::Error>
    where
        D: Deserializer<'de>,
    {
        unimplemented!();
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
