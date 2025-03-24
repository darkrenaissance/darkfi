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

use std::sync::{Arc, Mutex};

use lazy_static::lazy_static;
use smol::Executor;
use tempdir::TempDir;
use tinyjson::JsonValue;
use url::Url;

use darkfi::rpc::{
    jsonrpc::{ErrorCode, JsonRequest, JsonResult},
    server::RequestHandler,
};

use crate::Explorerd;

// Defines a global `Explorerd` instance shared across all tests
lazy_static! {
    static ref EXPLORERD_INSTANCE: Mutex<Option<Arc<Explorerd>>> = Mutex::new(None);
}

/// Initializes logging for test cases, which is useful for debugging issues encountered during testing.
/// The logger is configured based on the provided list of targets to ignore and the desired log level.
#[cfg(test)]
pub fn init_logger(log_level: simplelog::LevelFilter, ignore_targets: Vec<&str>) {
    let mut cfg = simplelog::ConfigBuilder::new();

    // Add targets to ignore
    for target in ignore_targets {
        cfg.add_filter_ignore(target.to_string());
    }

    // Set log level
    cfg.set_target_level(log_level);

    // initialize the logger
    if simplelog::TermLogger::init(
        log_level,
        cfg.build(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
    .is_err()
    {
        // Print an error message if logger failed to initialize
        eprintln!("Logger failed to initialize");
    }
}

#[cfg(test)]
/// Sets up the `Explorerd` instance for testing, ensuring a single instance is initialized only
/// once and shared among subsequent setup calls.
pub fn setup() -> Arc<Explorerd> {
    let mut instance = EXPLORERD_INSTANCE.lock().expect("Failed to lock EXPLORERD_INSTANCE mutex");

    if instance.is_none() {
        // Initialize logger for the first time
        init_logger(simplelog::LevelFilter::Off, vec!["sled", "runtime", "net"]);

        // Prepare parameters for Explorerd::new
        let temp_dir = TempDir::new("explorerd").expect("Failed to create temp dir");
        let db_path_buf = temp_dir.path().join("explorerd_0");
        let db_path =
            db_path_buf.to_str().expect("Failed to convert db_path to string").to_string();
        let darkfid_endpoint = Url::parse("http://127.0.0.1:8240").expect("Invalid URL");
        let executor = Arc::new(Executor::new());

        // Block on the async function to resolve Explorerd::new
        let explorerd = smol::block_on(Explorerd::new(db_path, darkfid_endpoint, executor))
            .expect("Failed to initialize Explorerd instance");

        // Store the initialized instance in the global Mutex
        *instance = Some(Arc::new(explorerd));
    }

    // Return a clone of the shared instance
    Arc::clone(instance.as_ref().unwrap())
}

/// Auxiliary function that validates the correct handling of an invalid JSON-RPC parameter. It
/// prepares a JSON-RPC request with the provided method and params. It then sends the request using
/// the [`Explorerd::handle_request`] function of the provided [`Explorerd`] instance. Verifies the
/// response is an error, matching the expected error code and message.
pub async fn validate_invalid_rpc_parameter(
    explorerd: &Explorerd,
    method_name: &str,
    params: &[JsonValue],
    expected_error_code: i32,
    expected_error_message: &str,
) {
    // Prepare an invalid JSON-RPC request with the provided `params`
    let request = JsonRequest {
        id: 1,
        jsonrpc: "2.0",
        method: method_name.to_string(),
        params: JsonValue::Array(params.to_vec()),
    };

    // Call `handle_request` on the Explorerd instance
    let response = explorerd.handle_request(request).await;

    // Verify response is a `JsonError` with the appropriate error code and message
    match response {
        JsonResult::Error(actual_error) => {
            assert_eq!(actual_error.error.message, expected_error_message);
            assert_eq!(actual_error.error.code, expected_error_code);
        }
        _ => panic!(
            "Expected a JSON error response for method: {}, but got something else",
            method_name
        ),
    }
}

/// Auxiliary function that validates the handling of non-empty parameters when they are supposed
/// to be empty for the given RPC `method`. It uses the provided [`Explorerd`] instance to ensure
/// that unexpected non-empty parameters result in the expected error for invalid parameters.
pub async fn validate_empty_rpc_parameters(explorerd: &Explorerd, method: &str) {
    // Prepare a JSON-RPC request for `ping_darkfid`
    let request = JsonRequest {
        id: 1,
        jsonrpc: "2.0",
        method: method.to_string(),
        params: JsonValue::Array(vec![JsonValue::String("non_empty_param".to_string())]),
    };

    // Call `handle_request` on the Explorerd instance.
    let response = explorerd.handle_request(request).await;

    // Verify the response is a `JsonError` with the `PingFailed` error code
    match response {
        JsonResult::Error(actual_error) => {
            let expected_error_code = ErrorCode::InvalidParams.code();
            let expected_error_msg =
                "Parameters not permited, received: \"[\\\"non_empty_param\\\"]\"";
            assert_eq!(actual_error.error.code, expected_error_code);
            assert_eq!(actual_error.error.message, expected_error_msg);
        }
        _ => panic!("Expected a JSON object for the response, but got something else"),
    }
}

/// Auxiliary function that validates the handling of an invalid contract ID when calling the specified
/// JSON-RPC method, ensuring appropriate error responses from provided [`Explorerd`].
pub fn validate_invalid_rpc_contract_id(explorerd: &Explorerd, method: &str) {
    validate_invalid_rpc_hash_parameter(explorerd, method, "contract_id", "Invalid contract ID");
}

/// Auxiliary function that validates the handling of an invalid header hash when calling the specified
/// JSON-RPC `method`, ensuring appropriate error responses from provided [`Explorerd`].
pub fn validate_invalid_rpc_header_hash(explorerd: &Explorerd, method: &str) {
    validate_invalid_rpc_hash_parameter(explorerd, method, "header_hash", "Invalid header hash");
}

/// Auxiliary function that validates the handling of an invalid tx hash when calling the specified JSON-RPC
/// `method`, ensuring appropriate error responses from provided [`Explorerd`].
pub fn validate_invalid_rpc_tx_hash(explorerd: &Explorerd, method: &str) {
    validate_invalid_rpc_hash_parameter(explorerd, method, "tx_hash", "Invalid tx hash");
}

/// Auxiliary function that validates the correct handling of invalid hash parameters
/// when calling the given RPC `method` using the provided [`Explorerd`]. This includes checks for
/// missing parameters, incorrect parameter types, and invalid hash values, ensuring it returns
/// error responses matching the expected error codes and messages.
fn validate_invalid_rpc_hash_parameter(
    explorerd: &Explorerd,
    method: &str,
    parameter_name: &str,
    invalid_hash_value_message: &str,
) {
    smol::block_on(async {
        // Test for missing `parameter_name` parameter
        validate_invalid_rpc_parameter(
            explorerd,
            method,
            &[],
            ErrorCode::InvalidParams.code(),
            &format!("Parameter '{}' at index 0 is missing", parameter_name),
        )
        .await;

        // Test for invalid `parameter_name` parameter type
        validate_invalid_rpc_parameter(
            explorerd,
            method,
            &[JsonValue::Number(123.0)],
            ErrorCode::InvalidParams.code(),
            &format!("Parameter '{}' is not a valid string", parameter_name),
        )
        .await;

        // Test for invalid `contract_id` value
        validate_invalid_rpc_parameter(
            explorerd,
            method,
            &[JsonValue::String("0x0222".to_string())],
            ErrorCode::InvalidParams.code(),
            &format!("{}: 0x0222", invalid_hash_value_message),
        )
        .await;
    });
}
