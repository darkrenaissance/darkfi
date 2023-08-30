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

/*
use log::{debug, error};
use serde_json::{json, Value};

use darkfi::{
    rpc::jsonrpc::{
        ErrorCode::{InternalError, InvalidParams, ParseError},
        JsonError, JsonResponse, JsonResult,
    },
    wallet::walletdb::QueryType,
};

use super::{error::RpcError, server_error, Darkfid};
*/
use darkfi::rpc::jsonrpc::JsonResult;
use tinyjson::JsonValue;

use super::Darkfid;

impl Darkfid {
    // RPCAPI:
    // Attempts to query for a single row in a given table.
    // The parameters given contain paired metadata so we know how to decode the SQL data.
    // An example of `params` is as such:
    // ```
    // params[0] -> "sql query"
    // params[1] -> column_type
    // params[2] -> "column_name"
    // ...
    // params[n-1] -> column_type
    // params[n] -> "column_name"
    // ```
    // This function will fetch the first row it finds, if any. The `column_type` field
    // is a type available in the `WalletDb` API as an enum called `QueryType`. If a row
    // is not found, the returned result will be a JSON-RPC error.
    // NOTE: This is obviously vulnerable to SQL injection. Open to interesting solutions.
    //
    // --> {"jsonrpc": "2.0", "method": "wallet.query_row_single", "params": [...], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["va", "lu", "es", ...], "id": 1}
    pub async fn wallet_query_row_single(&self, _id: u16, _params: JsonValue) -> JsonResult {
        todo!();
        /* TODO: This will be abstracted away
        // We need at least 3 params for something we want to fetch, and we want them in pairs.
        // Also the first param should be a String
        if params.len() < 3 || params[1..].len() % 2 != 0 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // The remaining pairs should be typed properly too
        let mut types: Vec<QueryType> = vec![];
        let mut names: Vec<&str> = vec![];
        for pair in params[1..].chunks(2) {
            if !pair[0].is_u64() || !pair[1].is_string() {
                return JsonError::new(InvalidParams, None, id).into()
            }

            let typ = pair[0].as_u64().unwrap();
            if typ >= QueryType::Last as u64 {
                return JsonError::new(InvalidParams, None, id).into()
            }

            types.push((typ as u8).into());
            names.push(pair[1].as_str().unwrap());
        }

        // Get a wallet connection
        let mut conn = match self.wallet.conn.acquire().await {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] wallet.query_row_single: Failed to acquire wallet connection: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        // Execute the query and see if we find a row
        let row = match sqlx::query(params[0].as_str().unwrap()).fetch_one(&mut conn).await {
            Ok(v) => Some(v),
            Err(_) => None,
        };

        // Try to decode the row into what was requested
        let mut ret: Vec<Value> = vec![];

        for (typ, col) in types.iter().zip(names) {
            match typ {
                QueryType::Integer => {
                    let Some(ref row) = row else {
                        error!("[RPC] wallet.query_row_single: Got None for QueryType::Integer");
                        return server_error(RpcError::NoRowsFoundInWallet, id, None)
                    };

                    let value: i32 = match row.try_get(col) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[RPC] wallet.query_row_single: {}", e);
                            return JsonError::new(ParseError, None, id).into()
                        }
                    };

                    ret.push(json!(value));
                    continue
                }

                QueryType::Blob => {
                    let Some(ref row) = row else {
                        error!("[RPC] wallet.query_row_single: Got None for QueryType::Blob");
                        return server_error(RpcError::NoRowsFoundInWallet, id, None)
                    };

                    let value: Vec<u8> = match row.try_get(col) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[RPC] wallet.query_row_single: {}", e);
                            return JsonError::new(ParseError, None, id).into()
                        }
                    };

                    ret.push(json!(value));
                    continue
                }

                QueryType::OptionInteger => {
                    let Some(ref row) = row else {
                        ret.push(json!(None::<i32>));
                        continue
                    };

                    let value: i32 = match row.try_get(col) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[RPC] wallet.query_row_single: {}", e);
                            return JsonError::new(ParseError, None, id).into()
                        }
                    };

                    ret.push(json!(value));
                    continue
                }

                QueryType::OptionBlob => {
                    let Some(ref row) = row else {
                        ret.push(json!(None::<Vec<u8>>));
                        continue
                    };

                    let value: Vec<u8> = match row.try_get(col) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[RPC] wallet.query_row_single: {}", e);
                            return JsonError::new(ParseError, None, id).into()
                        }
                    };

                    ret.push(json!(value));
                    continue
                }

                QueryType::Text => {
                    let Some(ref row) = row else {
                        error!("[RPC] wallet.query_row_single: Got None for QueryType::Text");
                        return server_error(RpcError::NoRowsFoundInWallet, id, None)
                    };

                    let value: String = match row.try_get(col) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[RPC] wallet.query_row_single: {}", e);
                            return JsonError::new(ParseError, None, id).into()
                        }
                    };

                    ret.push(json!(value));
                    continue
                }

                _ => unreachable!(),
            }
        }

        JsonResponse::new(json!(ret), id).into()
        */
    }

    // RPCAPI:
    // Attempts to query for all available rows in a given table.
    // The parameters given contain paired metadata so we know how to decode the SQL data.
    // They're the same as above in `wallet.query_row_single`.
    // If there are any values found, they will be returned in a paired array. If not, an
    // empty array will be returned.
    //
    // --> {"jsonrpc": "2.0", "method": "wallet.query_row_multi", "params": [...], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [["va", "lu"], ["es", "es"], ...], "id": 1}
    pub async fn wallet_query_row_multi(&self, _id: u16, _params: JsonValue) -> JsonResult {
        todo!();
        /* TODO: This will be abstracted away
        // We need at least 3 params for something we want to fetch, and we want them in pairs.
        // Also the first param (the query) should be a String.
        if params.len() < 3 || params[1..].len() % 2 != 0 || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // The remaining pairs should be typed properly too
        let mut types: Vec<QueryType> = vec![];
        let mut names: Vec<&str> = vec![];
        for pair in params[1..].chunks(2) {
            if !pair[0].is_u64() || !pair[1].is_string() {
                return JsonError::new(InvalidParams, None, id).into()
            }

            let typ = pair[0].as_u64().unwrap();
            if typ >= QueryType::Last as u64 {
                return JsonError::new(InvalidParams, None, id).into()
            }

            types.push((typ as u8).into());
            names.push(pair[1].as_str().unwrap());
        }

        // Get a wallet connection
        let mut conn = match self.wallet.conn.acquire().await {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] wallet.query_row_multi: Failed to acquire wallet connection: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        // Execute the query and see if we find any rows
        let rows = match sqlx::query(params[0].as_str().unwrap()).fetch_all(&mut conn).await {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] wallet.query_row_multi: Failed to execute SQL query: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        debug!("[RPC] wallet.query_row_multi: Found {} rows", rows.len());

        // Try to decode whatever we've found
        let mut ret: Vec<Vec<Value>> = vec![];

        for row in rows {
            let mut row_ret: Vec<Value> = vec![];
            for (typ, col) in types.iter().zip(names.clone()) {
                match typ {
                    QueryType::Integer => {
                        let value: i32 = match row.try_get(col) {
                            Ok(v) => v,
                            Err(e) => {
                                error!("[RPC] wallet.query_row_multi: {}", e);
                                return JsonError::new(ParseError, None, id).into()
                            }
                        };

                        row_ret.push(json!(value));
                    }

                    QueryType::Blob => {
                        let value: Vec<u8> = match row.try_get(col) {
                            Ok(v) => v,
                            Err(e) => {
                                error!("[RPC] wallet.query_row_multi: {}", e);
                                return JsonError::new(ParseError, None, id).into()
                            }
                        };

                        row_ret.push(json!(value));
                    }

                    QueryType::OptionInteger => {
                        let value: Option<i32> = match row.try_get(col) {
                            Ok(v) => Some(v),
                            Err(_) => None,
                        };

                        row_ret.push(json!(value));
                    }

                    QueryType::OptionBlob => {
                        let value: Option<Vec<u8>> = match row.try_get(col) {
                            Ok(v) => Some(v),
                            Err(_) => None,
                        };

                        row_ret.push(json!(value));
                    }

                    QueryType::Text => {
                        let value: String = match row.try_get(col) {
                            Ok(v) => v,
                            Err(e) => {
                                error!("[RPC] wallet.query_row_multi: {}", e);
                                return JsonError::new(ParseError, None, id).into()
                            }
                        };

                        row_ret.push(json!(value));
                    }

                    _ => unreachable!(),
                }
            }

            ret.push(row_ret);
        }

        JsonResponse::new(json!(ret), id).into()
        */
    }

    // RPCAPI:
    // Executes an arbitrary SQL query on the wallet, and returns `true` on success.
    // `params[1..]` can optionally be provided in pairs like in `wallet.query_row_single`.
    //
    // --> {"jsonrpc": "2.0", "method": "wallet.exec_sql", "params": ["CREATE TABLE ..."], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    pub async fn wallet_exec_sql(&self, _id: u16, _params: JsonValue) -> JsonResult {
        todo!();
        /* TODO: This will be abstracted away
        if params.is_empty() || !params[0].is_string() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        if params.len() > 1 && params[1..].len() % 2 != 0 {
            return JsonError::new(InvalidParams, None, id).into()
        }

        let query = params[0].as_str().unwrap();
        debug!("Executing SQL query: {}", query);
        let mut query = sqlx::query(query);

        for pair in params[1..].chunks(2) {
            if !pair[0].is_u64() || pair[0].as_u64().unwrap() >= QueryType::Last as u64 {
                return JsonError::new(InvalidParams, None, id).into()
            }

            let typ = (pair[0].as_u64().unwrap() as u8).into();
            match typ {
                QueryType::Integer => {
                    let val: i32 = match serde_json::from_value(pair[1].clone()) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[RPC] wallet.exec_sql: Failed casting value to i32: {}", e);
                            return JsonError::new(ParseError, None, id).into()
                        }
                    };

                    query = query.bind(val);
                }

                QueryType::Blob => {
                    let val: Vec<u8> = match serde_json::from_value(pair[1].clone()) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[RPC] wallet.exec_sql: Failed casting value to Vec<u8>: {}", e);
                            return JsonError::new(ParseError, None, id).into()
                        }
                    };

                    query = query.bind(val);
                }

                QueryType::OptionInteger => {
                    let val: Option<i32> = match serde_json::from_value(pair[1].clone()) {
                        Ok(v) => v,
                        Err(e) => {
                            error!(
                                "[RPC] wallet.exec_sql: Failed casting value to Option<i32>: {}",
                                e
                            );
                            return JsonError::new(ParseError, None, id).into()
                        }
                    };

                    query = query.bind(val);
                }

                QueryType::OptionBlob => {
                    let val: Option<Vec<u8>> = match serde_json::from_value(pair[1].clone()) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[RPC] wallet.exec_sql: Failed casting value to Option<Vec<u8>>: {}", e);
                            return JsonError::new(ParseError, None, id).into()
                        }
                    };

                    query = query.bind(val);
                }

                QueryType::Text => {
                    let val: String = match serde_json::from_value(pair[1].clone()) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[RPC] wallet.exec_sql: Failed casting value to String: {}", e);
                            return JsonError::new(ParseError, None, id).into()
                        }
                    };

                    query = query.bind(val);
                }

                _ => return JsonError::new(InvalidParams, None, id).into(),
            }
        }

        // Get a wallet connection
        let mut conn = match self.wallet.conn.acquire().await {
            Ok(v) => v,
            Err(e) => {
                error!("[RPC] wallet.exec_sql: Failed to acquire wallet connection: {}", e);
                return JsonError::new(InternalError, None, id).into()
            }
        };

        if let Err(e) = query.execute(&mut conn).await {
            error!("[RPC] wallet.exec_sql: Failed to execute sql query: {}", e);
            return JsonError::new(InternalError, None, id).into()
        };

        JsonResponse::new(json!(true), id).into()
        */
    }
}
