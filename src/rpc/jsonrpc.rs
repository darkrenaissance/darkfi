use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonRequest {
    jsonrpc: Value,
    method: Value,
    //params: Vec<Value>,
    params: Value,
    id: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonErrorVal {
    code: Value,
    message: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonError {
    jsonrpc: Value,
    error: JsonErrorVal,
    id: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonResponse {
    jsonrpc: Value,
    result: Value,
    id: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonNotification {
    jsonrpc: Value,
    method: Value,
    //params: Vec<Value>,
    params: Value,
}

pub fn request(m: Value, p: Value) -> JsonRequest {
    let mut rng = rand::thread_rng();

    return JsonRequest {
        jsonrpc: json!("2.0"),
        method: m,
        params: p,
        id: json!(rng.gen::<u32>()),
    };
}

pub fn response(r: Value, i: Value) -> JsonResponse {
    return JsonResponse {
        jsonrpc: json!("2.0"),
        result: r,
        id: i,
    };
}

pub fn error(c: i64, m: String, i: Value) -> JsonError {
    let ev = JsonErrorVal {
        code: json!(c),
        message: json!(m),
    };

    return JsonError {
        jsonrpc: json!("2.0"),
        error: ev,
        id: i,
    };
}

pub fn notification(m: Value, p: Value) -> JsonNotification {
    return JsonNotification {
        jsonrpc: json!("2.0"),
        method: m,
        params: p,
    };
}
