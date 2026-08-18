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

use serde::{Deserialize, Deserializer};
use structopt::StructOpt;
use url::Url;

fn default_listen_addrs() -> Vec<Url> {
    vec![Url::parse("tcp://127.0.0.1:22222").unwrap()]
}

/// Deserialize both the historical single URL and the preferred URL array.
///
/// Keeping the single-URL form working avoids invalidating existing daemon
/// configurations while allowing RPC to use the same array syntax as P2P.
fn deserialize_listen_addrs<'de, D>(deserializer: D) -> Result<Vec<Url>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum ListenAddrs {
        One(Url),
        Many(Vec<Url>),
    }

    Ok(match ListenAddrs::deserialize(deserializer)? {
        ListenAddrs::One(url) => vec![url],
        ListenAddrs::Many(urls) => urls,
    })
}

#[derive(Clone, Debug)]
pub struct RpcSettings {
    /// RPC server listen addresses.
    pub listen: Vec<Url>,
    pub disabled_methods: Vec<String>,
}

impl RpcSettings {
    pub fn is_method_disabled(&self, method: &String) -> bool {
        self.disabled_methods.contains(method)
    }

    pub fn use_http(&self) -> bool {
        self.listen.first().is_some_and(|endpoint| endpoint.scheme().starts_with("http+"))
    }
}

impl Default for RpcSettings {
    fn default() -> Self {
        Self { listen: default_listen_addrs(), disabled_methods: vec![] }
    }
}

// Defines the JSON-RPC settings.
#[derive(Clone, Debug, serde::Deserialize, structopt::StructOpt, structopt_toml::StructOptToml)]
#[structopt()]
#[serde(rename = "rpc")]
pub struct RpcSettingsOpt {
    /// RPC server listen addresses
    #[serde(default = "default_listen_addrs", deserialize_with = "deserialize_listen_addrs")]
    #[structopt(long, default_value = "tcp://127.0.0.1:22222", use_delimiter = true)]
    pub rpc_listen: Vec<Url>,

    /// Disabled JSON-RPC methods
    #[structopt(long, use_delimiter = true)]
    pub rpc_disabled_methods: Option<Vec<String>>,
}

impl From<RpcSettingsOpt> for RpcSettings {
    fn from(opt: RpcSettingsOpt) -> Self {
        Self {
            listen: opt.rpc_listen,
            disabled_methods: opt.rpc_disabled_methods.unwrap_or_default(),
        }
    }
}
