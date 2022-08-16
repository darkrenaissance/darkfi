use std::sync::Arc;

use async_trait::async_trait;
use log::debug;
use serde_json::{json, Value};
use simplelog::{ColorChoice, LevelFilter, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    rpc::{
        jsonrpc::{ErrorCode::*, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::{listen_and_serve, RequestHandler},
    },
    Result,
};

mod dao_contract;
mod demo;
mod money_contract;
mod note;
mod util;
use crate::demo::demo;

async fn _start() -> Result<()> {
    let rpc_addr = Url::parse("tcp://127.0.0.1:7777")?;
    let rpc_interface = Arc::new(JsonRpcInterface {});

    listen_and_serve(rpc_addr, rpc_interface).await?;
    Ok(())
}

struct JsonRpcInterface {}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonError::new(InvalidParams, None, req.id).into()
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        match req.method.as_str() {
            Some("say_hello") => return self.say_hello(req.id, req.params).await,
            Some(_) | None => return JsonError::new(MethodNotFound, None, req.id).into(),
        }
    }
}

impl JsonRpcInterface {
    // --> {"method": "say_hello", "params": []}
    // <-- {"result": "hello world"}
    async fn say_hello(&self, id: Value, _params: Value) -> JsonResult {
        JsonResponse::new(json!("hello world"), id).into()
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    TermLogger::init(
        LevelFilter::Debug,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )?;

    //start().await?;
    demo().await.unwrap();
    Ok(())
}
