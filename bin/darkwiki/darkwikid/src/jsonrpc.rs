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

use async_trait::async_trait;
use log::error;
use serde_json::{json, Value};

use darkfi::{
    rpc::{
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResponse, JsonResult},
        server::RequestHandler,
    },
    Error,
};

use crate::Patch;

pub struct JsonRpcInterface {
    sender: smol::channel::Sender<(String, bool, Vec<String>)>,
    receiver: smol::channel::Receiver<Vec<Vec<Patch>>>,
}

#[async_trait]
impl RequestHandler for JsonRpcInterface {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        if !req.params.is_array() {
            return JsonError::new(ErrorCode::InvalidParams, None, req.id).into()
        }

        let params = req.params.as_array().unwrap();

        let rep = match req.method.as_str() {
            Some("update") => self.update(req.id, params).await,
            Some("restore") => self.restore(req.id, params).await,
            Some("log") => self.log(req.id, params).await,
            Some(_) | None => return JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        };

        rep
    }
}

fn patch_to_tuple(p: &Patch, colorize: bool) -> (String, String, String) {
    (p.path.to_owned(), p.workspace.to_owned(), if colorize { p.colorize() } else { p.to_string() })
}

fn printable_patches(
    patches: Vec<Vec<Patch>>,
    colorize: bool,
) -> Vec<Vec<(String, String, String)>> {
    let mut response = vec![];
    for ps in patches {
        response.push(ps.iter().map(|p| patch_to_tuple(p, colorize)).collect())
    }
    response
}

impl JsonRpcInterface {
    pub fn new(
        sender: smol::channel::Sender<(String, bool, Vec<String>)>,
        receiver: smol::channel::Receiver<Vec<Vec<Patch>>>,
    ) -> Self {
        Self { sender, receiver }
    }

    // RPCAPI:
    // Update files
    // --> {"jsonrpc": "2.0", "method": "update", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [[(String, String)]], "id": 1}
    async fn update(&self, id: Value, params: &[Value]) -> JsonResult {
        let dry = params[0].as_bool().unwrap();
        let files: Vec<String> =
            params[1].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();
        let res = self.sender.send(("update".into(), dry, files)).await.map_err(Error::from);

        if let Err(e) = res {
            error!("Failed to update: {}", e);
            return JsonError::new(ErrorCode::InternalError, None, id).into()
        }

        let response = self.receiver.recv().await.unwrap();
        let response = printable_patches(response, true);
        JsonResponse::new(json!(response), id).into()
    }

    // RPCAPI:
    // Undo the local changes
    // --> {"jsonrpc": "2.0", "method": "restore", "params": [dry, files], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [String, ..], "id": 1}
    async fn restore(&self, id: Value, params: &[Value]) -> JsonResult {
        let dry = params[0].as_bool().unwrap();
        let files: Vec<String> =
            params[1].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();

        let res = self.sender.send(("restore".into(), dry, files)).await.map_err(Error::from);

        if let Err(e) = res {
            error!("Failed to restore: {}", e);
            return JsonError::new(ErrorCode::InternalError, None, id).into()
        }

        let response = self.receiver.recv().await.unwrap();
        let response = printable_patches(response, false);
        JsonResponse::new(json!(response), id).into()
    }

    // RPCAPI:
    // Show all patches
    // --> {"jsonrpc": "2.0", "method": "log", "params": [dry, files], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [[(String, String)]], "id": 1}
    async fn log(&self, id: Value, _params: &[Value]) -> JsonResult {
        JsonResponse::new(json!(true), id).into()
    }
}
