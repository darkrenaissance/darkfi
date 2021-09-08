use std::str;

use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
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

pub fn error(c: i64, m: String, i: Value) -> JsonError {
    let ev = JsonErrorVal {
        code: json!(c),
        message: json!(m),
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
