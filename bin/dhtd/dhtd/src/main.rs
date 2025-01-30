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

use std::collections::{HashMap, HashSet};

use async_std::sync::{Arc, RwLock};
use darkfi::{dht2::Dht, Result};
use url::Url;

/// Protocol implementations
mod proto;

//#[cfg(test)]
mod tests;

pub type DhtdPtr = Arc<RwLock<Dhtd>>;

pub struct Dhtd {
    pub dht: Dht,
    pub routing_table: HashMap<blake3::Hash, HashSet<Url>>,
}

fn main() -> Result<()> {
    Ok(())
}
