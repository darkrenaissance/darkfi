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

//! JSON-RPC 2.0 object definitions
use std::collections::HashMap;

use rand::{rngs::OsRng, Rng};
use tinyjson::JsonValue;

use crate::{
    error::RpcError,
    system::{Publisher, PublisherPtr},
    Result,
};

/// JSON-RPC error codes.
/// The error codes `[-32768, -32000]` are reserved for predefined errors.
#[derive(Copy, Clone, Debug)]
pub enum ErrorCode {
    /// Invalid JSON was received by the server.
    /// An error occurred on the server while parsing the JSON text.
    ParseError,
    /// The JSON sent is not a valid Request object.
    InvalidRequest,
    /// The method does not exist / is not available.
    MethodNotFound,
    /// Invalid method parameter(s).
    InvalidParams,
    /// Internal JSON-RPC error.
    InternalError,
    /// ID mismatch
    IdMismatch,
    /// Invalid/Unexpected reply
    InvalidReply,
    /// Reserved for implementation-defined server-errors.
    ServerError(i32),
}

impl ErrorCode {
    pub fn code(&self) -> i32 {
        match *self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
            Self::IdMismatch => -32360,
            Self::InvalidReply => -32361,
            Self::ServerError(c) => c,
        }
    }

    pub fn message(&self) -> String {
        match *self {
            Self::ParseError => "parse error".to_string(),
            Self::InvalidRequest => "invalid request".to_string(),
            Self::MethodNotFound => "method not found".to_string(),
            Self::InvalidParams => "invalid params".to_string(),
            Self::InternalError => "internal error".to_string(),
            Self::IdMismatch => "id mismatch".to_string(),
            Self::InvalidReply => "invalid reply".to_string(),
            Self::ServerError(_) => "server error".to_string(),
        }
    }

    pub fn desc(&self) -> JsonValue {
        JsonValue::String(self.message())
    }
}

// ANCHOR: jsonresult
/// Wrapping enum around the available JSON-RPC object types
#[derive(Clone, Debug)]
pub enum JsonResult {
    Response(JsonResponse),
    Error(JsonError),
    Notification(JsonNotification),
    /// Subscriber is a special object that yields a channel
    Subscriber(JsonSubscriber),
    SubscriberWithReply(JsonSubscriber, JsonResponse),
    Request(JsonRequest),
}

impl JsonResult {
    pub fn try_from_value(value: &JsonValue) -> Result<Self> {
        if let Ok(response) = JsonResponse::try_from(value) {
            return Ok(Self::Response(response))
        }

        if let Ok(error) = JsonError::try_from(value) {
            return Ok(Self::Error(error))
        }

        if let Ok(notification) = JsonNotification::try_from(value) {
            return Ok(Self::Notification(notification))
        }

        Err(RpcError::InvalidJson("Invalid JSON Result".to_string()).into())
    }
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

impl From<JsonSubscriber> for JsonResult {
    fn from(sub: JsonSubscriber) -> Self {
        Self::Subscriber(sub)
    }
}

impl From<(JsonSubscriber, JsonResponse)> for JsonResult {
    fn from(tuple: (JsonSubscriber, JsonResponse)) -> Self {
        Self::SubscriberWithReply(tuple.0, tuple.1)
    }
}

// ANCHOR: jsonrequest
/// A JSON-RPC request object
#[derive(Clone, Debug)]
pub struct JsonRequest {
    /// JSON-RPC version
    pub jsonrpc: &'static str,
    /// Request ID
    pub id: u16,
    /// Request method
    pub method: String,
    /// Request parameters
    pub params: JsonValue,
}
// ANCHOR_END: jsonrequest

impl JsonRequest {
    /// Create a new [`JsonRequest`] object with the given method and parameters.
    /// The request ID is chosen randomly.
    pub fn new(method: &str, params: JsonValue) -> Self {
        assert!(params.is_object() || params.is_array());
        Self { jsonrpc: "2.0", id: OsRng::gen(&mut OsRng), method: method.to_string(), params }
    }

    /// Convert the object into a JSON string
    pub fn stringify(&self) -> Result<String> {
        let v: JsonValue = self.into();
        Ok(v.stringify()?)
    }
}

impl From<&JsonRequest> for JsonValue {
    fn from(req: &JsonRequest) -> JsonValue {
        JsonValue::Object(HashMap::from([
            ("jsonrpc".to_string(), JsonValue::String(req.jsonrpc.to_string())),
            ("id".to_string(), JsonValue::Number(req.id.into())),
            ("method".to_string(), JsonValue::String(req.method.clone())),
            ("params".to_string(), req.params.clone()),
        ]))
    }
}

impl TryFrom<&JsonValue> for JsonRequest {
    type Error = RpcError;

    fn try_from(value: &JsonValue) -> std::result::Result<Self, Self::Error> {
        if !value.is_object() {
            return Err(RpcError::InvalidJson("JSON is not an Object".to_string()))
        }

        let map: &HashMap<String, JsonValue> = value.get().unwrap();

        if !map.contains_key("jsonrpc") ||
            !map["jsonrpc"].is_string() ||
            map["jsonrpc"] != JsonValue::String("2.0".to_string())
        {
            return Err(RpcError::InvalidJson(
                "Request does not contain valid \"jsonrpc\" field".to_string(),
            ))
        }

        if !map.contains_key("id") || !map["id"].is_number() {
            return Err(RpcError::InvalidJson(
                "Request does not contain valid \"id\" field".to_string(),
            ))
        }

        if !map.contains_key("method") || !map["method"].is_string() {
            return Err(RpcError::InvalidJson(
                "Request does not contain valid \"method\" field".to_string(),
            ))
        }

        if !map.contains_key("params") {
            return Err(RpcError::InvalidJson(
                "Request does not contain valid \"params\" field".to_string(),
            ))
        }

        if !map["params"].is_object() && !map["params"].is_array() {
            return Err(RpcError::InvalidJson(
                "Request does not contain valid \"params\" field".to_string(),
            ))
        }

        Ok(Self {
            jsonrpc: "2.0",
            id: *map["id"].get::<f64>().unwrap() as u16,
            method: map["method"].get::<String>().unwrap().clone(),
            params: map["params"].clone(),
        })
    }
}

/// A JSON-RPC notification object
#[derive(Clone, Debug)]
pub struct JsonNotification {
    /// JSON-RPC version
    pub jsonrpc: &'static str,
    /// Notification method
    pub method: String,
    /// Notification parameters
    pub params: JsonValue,
}

impl JsonNotification {
    /// Create a new [`JsonNotification`] object with the given method and parameters.
    pub fn new(method: &str, params: JsonValue) -> Self {
        assert!(params.is_object() || params.is_array());
        Self { jsonrpc: "2.0", method: method.to_string(), params }
    }

    /// Convert the object into a JSON string
    pub fn stringify(&self) -> Result<String> {
        let v: JsonValue = self.into();
        Ok(v.stringify()?)
    }
}

impl From<&JsonNotification> for JsonValue {
    fn from(notif: &JsonNotification) -> JsonValue {
        JsonValue::Object(HashMap::from([
            ("jsonrpc".to_string(), JsonValue::String(notif.jsonrpc.to_string())),
            ("method".to_string(), JsonValue::String(notif.method.clone())),
            ("params".to_string(), notif.params.clone()),
        ]))
    }
}

impl TryFrom<&JsonValue> for JsonNotification {
    type Error = RpcError;

    fn try_from(value: &JsonValue) -> std::result::Result<Self, Self::Error> {
        if !value.is_object() {
            return Err(RpcError::InvalidJson("JSON is not an Object".to_string()))
        }

        let map: &HashMap<String, JsonValue> = value.get().unwrap();

        if !map.contains_key("jsonrpc") ||
            !map["jsonrpc"].is_string() ||
            map["jsonrpc"] != JsonValue::String("2.0".to_string())
        {
            return Err(RpcError::InvalidJson(
                "Notification does not contain valid \"jsonrpc\" field".to_string(),
            ))
        }

        if !map.contains_key("method") || !map["method"].is_string() {
            return Err(RpcError::InvalidJson(
                "Notification does not contain valid \"method\" field".to_string(),
            ))
        }

        if !map.contains_key("params") {
            return Err(RpcError::InvalidJson(
                "Notification does not contain valid \"params\" field".to_string(),
            ))
        }

        if !map["params"].is_object() && !map["params"].is_array() {
            return Err(RpcError::InvalidJson(
                "Request does not contain valid \"params\" field".to_string(),
            ))
        }

        Ok(Self {
            jsonrpc: "2.0",
            method: map["method"].get::<String>().unwrap().clone(),
            params: map["params"].clone(),
        })
    }
}

/// A JSON-RPC response object
#[derive(Clone, Debug)]
pub struct JsonResponse {
    /// JSON-RPC version
    pub jsonrpc: &'static str,
    /// Request ID
    pub id: u16,
    /// Response result
    pub result: JsonValue,
}

impl JsonResponse {
    /// Create a new [`JsonResponse`] object with the given ID and result value.
    /// Creating a `JsonResponse` implies that the method call was successful.
    pub fn new(result: JsonValue, id: u16) -> Self {
        Self { jsonrpc: "2.0", id, result }
    }

    /// Convert the object into a JSON string
    pub fn stringify(&self) -> Result<String> {
        let v: JsonValue = self.into();
        Ok(v.stringify()?)
    }
}

impl From<&JsonResponse> for JsonValue {
    fn from(rep: &JsonResponse) -> JsonValue {
        JsonValue::Object(HashMap::from([
            ("jsonrpc".to_string(), JsonValue::String(rep.jsonrpc.to_string())),
            ("id".to_string(), JsonValue::Number(rep.id.into())),
            ("result".to_string(), rep.result.clone()),
        ]))
    }
}

impl TryFrom<&JsonValue> for JsonResponse {
    type Error = RpcError;

    fn try_from(value: &JsonValue) -> std::result::Result<Self, Self::Error> {
        if !value.is_object() {
            return Err(RpcError::InvalidJson("Json is not an Object".to_string()))
        }

        let map: &HashMap<String, JsonValue> = value.get().unwrap();

        if !map.contains_key("jsonrpc") ||
            !map["jsonrpc"].is_string() ||
            map["jsonrpc"] != JsonValue::String("2.0".to_string())
        {
            return Err(RpcError::InvalidJson(
                "Response does not contain valid \"jsonrpc\" field".to_string(),
            ))
        }

        if !map.contains_key("id") || !map["id"].is_number() {
            return Err(RpcError::InvalidJson(
                "Response does not contain valid \"id\" field".to_string(),
            ))
        }

        if !map.contains_key("result") {
            return Err(RpcError::InvalidJson(
                "Response does not contain valid \"result\" field".to_string(),
            ))
        }

        Ok(Self {
            jsonrpc: "2.0",
            id: *map["id"].get::<f64>().unwrap() as u16,
            result: map["result"].clone(),
        })
    }
}

impl TryFrom<JsonResult> for JsonResponse {
    type Error = RpcError;

    /// Converts [`JsonResult`] to [`JsonResponse`], returning the response or an `InvalidJson`
    /// error if the structure is not a `JsonResponse`.
    fn try_from(result: JsonResult) -> std::result::Result<Self, Self::Error> {
        match result {
            JsonResult::Response(response) => Ok(response),
            _ => Err(RpcError::InvalidJson("Not a JsonResult::Response".to_string())),
        }
    }
}

/// A JSON-RPC error object
#[derive(Clone, Debug)]
pub struct JsonError {
    /// JSON-RPC version
    pub jsonrpc: &'static str,
    /// Request ID
    pub id: u16,
    /// JSON-RPC error (code and message)
    pub error: JsonErrorVal,
}

/// A JSON-RPC error value (code and message)
#[derive(Clone, Debug)]
pub struct JsonErrorVal {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
}

impl JsonError {
    /// Create a new [`JsonError`] object with the given error code, optional
    /// message, and a response ID.
    /// Creating a `JsonError` implies that the method call was unsuccessful.
    pub fn new(c: ErrorCode, message: Option<String>, id: u16) -> Self {
        let error = JsonErrorVal { code: c.code(), message: message.unwrap_or(c.message()) };
        Self { jsonrpc: "2.0", id, error }
    }

    /// Convert the object into a JSON string
    pub fn stringify(&self) -> Result<String> {
        let v: JsonValue = self.into();
        Ok(v.stringify()?)
    }
}

impl From<&JsonError> for JsonValue {
    fn from(err: &JsonError) -> JsonValue {
        let errmap = JsonValue::Object(HashMap::from([
            ("code".to_string(), JsonValue::Number(err.error.code.into())),
            ("message".to_string(), JsonValue::String(err.error.message.clone())),
        ]));

        JsonValue::Object(HashMap::from([
            ("jsonrpc".to_string(), JsonValue::String(err.jsonrpc.to_string())),
            ("id".to_string(), JsonValue::Number(err.id.into())),
            ("error".to_string(), errmap),
        ]))
    }
}

impl TryFrom<JsonResult> for JsonError {
    type Error = RpcError;

    /// Converts [`JsonResult`] to [`JsonError`], returning the response or an `InvalidJson`
    /// error if the structure is not a `JsonError`.
    fn try_from(result: JsonResult) -> std::result::Result<Self, Self::Error> {
        match result {
            JsonResult::Error(error) => Ok(error),
            _ => Err(RpcError::InvalidJson("Not a JsonResult::Error".to_string())),
        }
    }
}

impl TryFrom<&JsonValue> for JsonError {
    type Error = RpcError;

    fn try_from(value: &JsonValue) -> std::result::Result<Self, Self::Error> {
        if !value.is_object() {
            return Err(RpcError::InvalidJson("JSON is not an Object".to_string()))
        }

        let map: &HashMap<String, JsonValue> = value.get().unwrap();

        if !map.contains_key("jsonrpc") ||
            !map["jsonrpc"].is_string() ||
            map["jsonrpc"] != JsonValue::String("2.0".to_string())
        {
            return Err(RpcError::InvalidJson(
                "Error does not contain valid \"jsonrpc\" field".to_string(),
            ))
        }

        if !map.contains_key("id") || !map["id"].is_number() {
            return Err(RpcError::InvalidJson(
                "Error does not contain valid \"id\" field".to_string(),
            ))
        }

        if !map.contains_key("error") || !map["error"].is_object() {
            return Err(RpcError::InvalidJson(
                "Error does not contain valid \"error\" field".to_string(),
            ))
        }

        if !map["error"]["code"].is_number() {
            return Err(RpcError::InvalidJson(
                "Error does not contain valid \"error.code\" field".to_string(),
            ))
        }

        if !map["error"]["message"].is_string() {
            return Err(RpcError::InvalidJson(
                "Error does not contain valid \"error.message\" field".to_string(),
            ))
        }

        Ok(Self {
            jsonrpc: "2.0",
            id: *map["id"].get::<f64>().unwrap() as u16,
            error: JsonErrorVal {
                code: *map["error"]["code"].get::<f64>().unwrap() as i32,
                message: map["error"]["message"].get::<String>().unwrap().to_string(),
            },
        })
    }
}

/// A JSON-RPC subscriber for notifications
#[derive(Clone, Debug)]
pub struct JsonSubscriber {
    /// Notification method
    pub method: &'static str,
    /// Notification publisher
    pub publisher: PublisherPtr<JsonNotification>,
}

impl JsonSubscriber {
    pub fn new(method: &'static str) -> Self {
        let publisher = Publisher::new();
        Self { method, publisher }
    }

    /// Send a notification to the publisher with the given JSON object
    pub async fn notify(&self, params: JsonValue) {
        let notification = JsonNotification::new(self.method, params);
        self.publisher.notify(notification).await;
    }
}

/// Parses a [`JsonValue`] parameter into a `String`.
/// Returns the string if successful or an error if the value is not a valid string.
pub fn parse_json_string(name: &str, value: &JsonValue) -> std::result::Result<String, RpcError> {
    value
        .get::<String>()
        .cloned()
        .ok_or_else(|| RpcError::InvalidJson(format!("Parameter '{name}' is not a valid string")))
}

/// Parses a [`JsonValue`] parameter into a `f64`.
/// Returns the number if successful or an error if the value is not a valid number.
pub fn parse_json_number(name: &str, value: &JsonValue) -> std::result::Result<f64, RpcError> {
    value.get::<f64>().cloned().ok_or_else(|| {
        RpcError::InvalidJson(format!("Parameter '{name}' is not a supported number type"))
    })
}

/// Parses the element at the specified index in a [`JsonValue::Array`] into a
/// string. Returns the string if successful, or an error if the parameter is
/// missing, not an array, or not a valid string.
pub fn parse_json_array_string(
    name: &str,
    index: usize,
    array_value: &JsonValue,
) -> std::result::Result<String, RpcError> {
    match array_value {
        JsonValue::Array(values) => values
            .get(index)
            .ok_or_else(|| {
                RpcError::InvalidJson(format!("Parameter '{name}' at index {index} is missing"))
            })
            .and_then(|param| parse_json_string(name, param)),
        _ => Err(RpcError::InvalidJson(format!("Parameter '{name}' is not an array"))),
    }
}

/// Parses the element at the specified index in a [`JsonValue::Array`] into an
/// `f64` (compatible with [`JsonValue::Number`]). Returns the number if successful,
/// or an error if the parameter is missing, not an array, or is not a valid number.
pub fn parse_json_array_number(
    name: &str,
    index: usize,
    array_value: &JsonValue,
) -> std::result::Result<f64, RpcError> {
    match array_value {
        JsonValue::Array(values) => values
            .get(index)
            .ok_or_else(|| {
                RpcError::InvalidJson(format!("Parameter '{name}' at index {index} is missing"))
            })
            .and_then(|param| parse_json_number(name, param)),
        _ => Err(RpcError::InvalidJson(format!("Parameter '{name}' is not an array"))),
    }
}

/// Attempts to parse a `JsonResult`, converting it into a `JsonResponse` and
/// extracting a string result from it. Returns an error if conversion or
/// extraction fails, and the extracted string on success.
pub fn parse_json_response_string(
    json_result: JsonResult,
) -> std::result::Result<String, RpcError> {
    // Try converting `JsonResult` into a `JsonResponse`.
    let json_response: JsonResponse = json_result.try_into().map_err(|_| {
        RpcError::InvalidJson("Failed to convert JsonResult into JsonResponse".to_string())
    })?;

    // Attempt to extract a string result from the JsonResponse
    json_response.result.get::<String>().map(|value| value.to_string()).ok_or_else(|| {
        RpcError::InvalidJson("Failed to parse string from JsonResponse result".to_string())
    })
}

/// Converts the provided JSON-RPC parameters into an array of JSON values,
/// returning a reference to the array if successful, or a JsonResult error containing a
/// JsonError when the input is not a JSON array.
pub fn to_json_array(params: &JsonValue) -> std::result::Result<&Vec<JsonValue>, RpcError> {
    if let JsonValue::Array(array) = params {
        Ok(array)
    } else {
        Err(RpcError::InvalidJson(
            "Expected an array of values, but received a different JSON type.".to_string(),
        ))
    }
}

/// Validates whether the provided JSON parameter is an empty array or object, returning success if it is empty or an Error if it contains values.
pub fn validate_empty_params(params: &JsonValue) -> std::result::Result<(), RpcError> {
    match to_json_array(params) {
        Ok(array) if array.is_empty() => Ok(()),
        Ok(_) => Err(RpcError::InvalidJson(format!(
            "Parameters not permited, received: {:?}",
            params.stringify().unwrap_or("Error converting JSON to string".to_string())
        ))),
        Err(err) => Err(RpcError::InvalidJson(err.to_string())),
    }
}
