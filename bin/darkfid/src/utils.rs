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

use log::{debug, error};
use smol::fs::read_to_string;
use structopt_toml::StructOptToml;

use darkfi::{util::path::get_config_path, Error, Result};

use crate::{BlockchainNetwork, CONFIG_FILE};

/// Auxiliary function to parse darkfid configuration file and extract requested
/// blockchain network config.
pub async fn parse_blockchain_config(
    config: Option<String>,
    network: &str,
) -> Result<BlockchainNetwork> {
    // Grab config path
    let config_path = get_config_path(config, CONFIG_FILE)?;
    debug!(target: "darkfid", "Parsing configuration file: {:?}", config_path);

    // Parse TOML file contents
    let contents = read_to_string(&config_path).await?;
    let contents: toml::Value = match toml::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            error!(target: "darkfid", "Failed parsing TOML config: {}", e);
            return Err(Error::ParseFailed("Failed parsing TOML config"))
        }
    };

    // Grab requested network config
    let Some(table) = contents.as_table() else { return Err(Error::ParseFailed("TOML not a map")) };
    let Some(network_configs) = table.get("network_config") else {
        return Err(Error::ParseFailed("TOML does not contain network configurations"))
    };
    let Some(network_configs) = network_configs.as_table() else {
        return Err(Error::ParseFailed("`network_config` not a map"))
    };
    let Some(network_config) = network_configs.get(network) else {
        return Err(Error::ParseFailed("TOML does not contain requested network configuration"))
    };
    let network_config = toml::to_string(&network_config).unwrap();
    let network_config =
        match BlockchainNetwork::from_iter_with_toml::<Vec<String>>(&network_config, vec![]) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "darkfid", "Failed parsing requested network configuration: {}", e);
                return Err(Error::ParseFailed("Failed parsing requested network configuration"))
            }
        };
    debug!(target: "darkfid", "Parsed network configuration: {:?}", network_config);

    Ok(network_config)
}
