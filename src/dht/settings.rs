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

use structopt::StructOpt;

#[derive(Clone, Debug)]
pub struct DhtSettings {
    /// Number of nodes in a bucket
    pub k: usize,
    /// Number of lookup requests in a burst
    pub alpha: usize,
    /// Maximum number of parallel lookup requests
    pub concurrency: usize,
    /// Timeout in seconds
    pub timeout: u64,
}

impl Default for DhtSettings {
    fn default() -> Self {
        Self { k: 16, alpha: 4, concurrency: 10, timeout: 5 }
    }
}

#[derive(Clone, Debug, serde::Deserialize, structopt::StructOpt, structopt_toml::StructOptToml)]
#[structopt()]
#[serde(rename = "dht")]
pub struct DhtSettingsOpt {
    /// Number of nodes in a DHT bucket
    #[structopt(long)]
    pub dht_k: Option<usize>,

    /// Number of DHT lookup requests in a burst
    #[structopt(long)]
    pub dht_alpha: Option<usize>,

    /// Maximum number of parallel DHT lookup requests
    #[structopt(long)]
    pub dht_concurrency: Option<usize>,

    /// Timeout in seconds
    #[structopt(long)]
    pub dht_timeout: Option<u64>,
}

impl From<DhtSettingsOpt> for DhtSettings {
    fn from(opt: DhtSettingsOpt) -> Self {
        let def = DhtSettings::default();

        Self {
            k: opt.dht_k.unwrap_or(def.k),
            alpha: opt.dht_alpha.unwrap_or(def.alpha),
            concurrency: opt.dht_concurrency.unwrap_or(def.concurrency),
            timeout: opt.dht_timeout.unwrap_or(def.timeout),
        }
    }
}
