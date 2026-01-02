/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Deserialize;
use structopt::StructOpt;
use tracing::{debug, error};
use url::Url;

use darkfi::{rpc::settings::RpcSettingsOpt, util::file::load_file, Error, Result};

/// Represents an explorer configuration
#[derive(Clone, Debug, Deserialize, StructOpt)]
pub struct ExplorerConfig {
    /// Current active network
    #[allow(dead_code)] // Part of the config file
    pub network: String,
    /// Supported network configurations
    pub network_config: NetworkConfigs,
    /// Path to the configuration if read from a file
    pub path: Option<String>,
}

impl ExplorerConfig {
    /// Creates a new configuration from a given file path.
    /// If the file cannot be loaded or parsed, an error is returned.
    pub fn new(config_path: String) -> Result<Self> {
        // Load the configuration file from the specified path
        let config_content = load_file(Path::new(&config_path)).map_err(|err| {
            Error::ConfigError(format!(
                "Failed to read the configuration file {config_path}: {err:?}"
            ))
        })?;

        // Parse the loaded content into a configuration instance
        let mut config = toml::from_str::<Self>(&config_content).map_err(|e| {
            error!(target: "explorerd::config", "Failed parsing TOML config: {e}");
            Error::ConfigError(format!("Failed to parse the configuration file {config_path}"))
        })?;

        // Set the configuration path
        config.path = Some(config_path);

        debug!(target: "explorerd::config", "Successfully loaded configuration: {config:?}");

        Ok(config)
    }

    /// Returns the currently active network configuration.
    #[allow(dead_code)] // Test case currently using
    pub fn active_network_config(&self) -> Option<ExplorerNetworkConfig> {
        self.get_network_config(self.network.as_str())
    }

    /// Returns the network configuration for specified network.
    pub fn get_network_config(&self, network: &str) -> Option<ExplorerNetworkConfig> {
        match network {
            "localnet" => self.network_config.localnet.clone(),
            "testnet" => self.network_config.testnet.clone(),
            "mainnet" => self.network_config.mainnet.clone(),
            _ => None,
        }
    }
}

/// Provides a default `ExplorerConfig` configuration using the `testnet` network.
impl Default for ExplorerConfig {
    fn default() -> Self {
        Self {
            network: String::from("testnet"),
            network_config: NetworkConfigs::default(),
            path: None,
        }
    }
}

/// Attempts to convert a [`PathBuf`] to an [`ExplorerConfig`] by
/// loading and parsing from specified file path.
impl TryFrom<&PathBuf> for ExplorerConfig {
    type Error = Error;
    fn try_from(path: &PathBuf) -> Result<Self> {
        let path_str = path.to_str().ok_or_else(|| {
            Error::ConfigError("Unable to convert PathBuf to a valid UTF-8 path string".to_string())
        })?;

        // Create configuration and return
        ExplorerConfig::new(path_str.to_string())
    }
}

/// Deserializes a `&str` containing explorer content in TOML format into an [`ExplorerConfig`] instance.
impl FromStr for ExplorerConfig {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let config: ExplorerConfig =
            toml::from_str(s).map_err(|e| format!("Failed to parse ExplorerdConfig: {e}"))?;
        Ok(config)
    }
}

/// Represents network configurations for localnet, testnet, and mainnet.
#[derive(Debug, Clone, Deserialize, StructOpt)]
pub struct NetworkConfigs {
    /// Local network configuration
    pub localnet: Option<ExplorerNetworkConfig>,
    /// Testnet network configuration
    pub testnet: Option<ExplorerNetworkConfig>,
    /// Mainnet network configuration
    pub mainnet: Option<ExplorerNetworkConfig>,
}

/// Provides a default `NetworkConfigs` configuration using the `testnet` network.
impl Default for NetworkConfigs {
    fn default() -> Self {
        NetworkConfigs {
            localnet: None,
            testnet: Some(ExplorerNetworkConfig::default()),
            mainnet: None,
        }
    }
}

/// Deserializes a `&str` containing network configs content in TOML format into an [`NetworkConfigs`] instance.
impl FromStr for NetworkConfigs {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let config: NetworkConfigs =
            toml::from_str(s).map_err(|e| format!("Failed to parse NetworkConfigs: {e}"))?;
        Ok(config)
    }
}

/// Struct representing the configuration for an explorer network.
#[derive(Clone, Deserialize, StructOpt)]
#[structopt()]
#[serde(default)]
pub struct ExplorerNetworkConfig {
    #[structopt(flatten)]
    /// JSON-RPC settings used to set up a server that the explorer listens on for incoming RPC requests.
    pub rpc: RpcSettingsOpt,

    #[structopt(long, default_value = "~/.local/share/darkfi/explorerd/testnet")]
    /// Path to the explorer's database.
    pub database: String,

    #[structopt(short, long, default_value = "tcp://127.0.0.1:28345")]
    /// Endpoint of the DarkFi node JSON-RPC server to sync with.
    pub endpoint: Url,
}

/// Attempts to convert a tuple `(PathBuf, &str)` representing a configuration file path
/// and network name into an `ExplorerNetworkConfig`.
impl TryFrom<(&PathBuf, &String)> for ExplorerNetworkConfig {
    type Error = Error;
    fn try_from(path_and_network: (&PathBuf, &String)) -> Result<Self> {
        // Load the ExplorerConfig from the given file path
        let config: ExplorerConfig = path_and_network.0.try_into()?;
        // Retrieve the network configuration for the specified network
        match config.get_network_config(path_and_network.1) {
            Some(config) => Ok(config),
            None => Err(Error::ConfigError(format!(
                "Failed to retrieve network configuration for network: {}",
                path_and_network.1
            ))),
        }
    }
}

/// Provides a default `ExplorerNetworkConfig` instance using `structopt` default values defined
/// in the `ExplorerNetworkConfig` struct.
impl Default for ExplorerNetworkConfig {
    fn default() -> Self {
        Self::from_iter(&[""])
    }
}

/// Provides a user-friendly debug view of the `ExplorerdNetworkConfig` configuration.
impl fmt::Debug for ExplorerNetworkConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug_struct = f.debug_struct("ExplorerdConfig");
        debug_struct
            .field("rpc_listen", &self.rpc.rpc_listen.to_string().trim_end_matches('/'))
            .field("db_path", &self.database)
            .field("endpoint", &self.endpoint.to_string().trim_end_matches('/'));
        debug_struct.finish()
    }
}

/// Deserializes a `&str` containing network config content in TOML format into an [`ExplorerNetworkConfig`] instance.
impl FromStr for ExplorerNetworkConfig {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let config: ExplorerNetworkConfig = toml::from_str(s)
            .map_err(|e| format!("Failed to parse ExplorerdNetworkConfig: {e}"))?;
        Ok(config)
    }
}

#[cfg(test)]
/// Contains test cases for validating the functionality and correctness of the `ExplorerConfig`
/// and related components using a configuration loaded from a TOML file.
mod tests {
    use std::path::Path;

    use darkfi::util::logger::{setup_test_logger, Level};
    use tracing::warn;

    use super::*;

    /// Validates the functionality of initializing and interacting with `ExplorerConfig`
    /// loaded from a TOML file, ensuring correctness of the network-specific configurations.
    #[test]
    fn test_explorerd_config_from_file() {
        // Constants for expected configurations
        const CONFIG_PATH: &str = "explorerd_config.toml";
        const ACTIVE_NETWORK: &str = "testnet";

        const NETWORK_CONFIGS: &[(&str, &str, &str, &str)] = &[
            (
                "localnet",
                "~/.local/share/darkfi/explorerd/localnet",
                "tcp://127.0.0.1:28345/",
                "tcp://127.0.0.1:14567/",
            ),
            (
                "testnet",
                "~/.local/share/darkfi/explorerd/testnet",
                "tcp://127.0.0.1:18345/",
                "tcp://127.0.0.1:14667/",
            ),
            (
                "mainnet",
                "~/.local/share/darkfi/explorerd/mainnet",
                "tcp://127.0.0.1:8345/",
                "tcp://127.0.0.1:14767/",
            ),
        ];

        if setup_test_logger(
            &["sled", "runtime", "net"],
            false,
            Level::Info,
            //Level::Verbose,
            //Level::Debug,
            //Level::Trace,
        )
        .is_err()
        {
            warn!("Logger already initialized");
        }

        // Ensure the configuration file exists
        assert!(Path::new(CONFIG_PATH).exists());

        // Load the configuration
        let config = ExplorerConfig::new(CONFIG_PATH.to_string())
            .expect("Failed to load configuration from file");

        // Validate the expected network
        assert_eq!(config.network, ACTIVE_NETWORK);

        // Validate the path is correctly set
        assert_eq!(config.path.as_deref(), Some(CONFIG_PATH));

        // Validate that `active_network_config` correctly retrieves the testnet configuration
        let active_config = config.active_network_config();
        assert!(active_config.is_some(), "Active network configuration should not be None.");
        let active_config = active_config.unwrap();
        assert_eq!(active_config.database, NETWORK_CONFIGS[1].1); // Testnet database
        assert_eq!(active_config.endpoint.to_string(), NETWORK_CONFIGS[1].2);
        assert_eq!(&active_config.rpc.rpc_listen.to_string(), NETWORK_CONFIGS[1].3);

        // Validate all network configurations values (localnet, testnet, mainnet)
        for &(network, expected_db, expected_endpoint, expected_rpc) in NETWORK_CONFIGS {
            let network_config = config.get_network_config(network);

            if let Some(config) = network_config {
                assert_eq!(config.database, expected_db);
                assert_eq!(config.endpoint.to_string(), expected_endpoint);
                assert_eq!(config.rpc.rpc_listen.to_string(), expected_rpc);
            } else {
                assert!(network_config.is_none(), "{network} configuration is missing");
            }
        }

        // Validate (path, network).try_into()
        let config_path_buf = &PathBuf::from(CONFIG_PATH);
        let mainnet_string = &String::from("mainnet");
        let mainnet_config: ExplorerNetworkConfig = (config_path_buf, mainnet_string)
            .try_into()
            .expect("Failed to load explorer network config");
        assert_eq!(mainnet_config.database, NETWORK_CONFIGS[2].1); // Mainnet database
        assert_eq!(mainnet_config.endpoint.to_string(), NETWORK_CONFIGS[2].2);
        assert_eq!(&mainnet_config.rpc.rpc_listen.to_string(), NETWORK_CONFIGS[2].3);
    }
}
