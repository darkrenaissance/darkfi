/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 * Copyright (C) 2021 The Tari Project (BSD-3)
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

use std::{collections::HashMap, sync::Arc, time::Instant};

use randomx::{RandomXCache, RandomXFlags, RandomXVM};
use tracing::{debug, warn};

use crate::Result;

/// Atomic pointer to a RandomX light mode virtual machine instance.
pub type RandomXVMInstance = Arc<RandomXVM>;

/// The RandomX factory that manages the creation of RandomX VMs.
#[derive(Clone, Debug)]
pub struct RandomXFactory {
    vms: HashMap<Vec<u8>, (Instant, RandomXVMInstance)>,
    max_vms: usize,
}

impl Default for RandomXFactory {
    fn default() -> Self {
        Self::new(2)
    }
}

impl RandomXFactory {
    /// Create a new RandomXFactory with the specified maximum number
    /// of VMs.
    pub fn new(max_vms: usize) -> Self {
        Self { vms: HashMap::new(), max_vms }
    }

    /// Create a new RandomX VM instance with the specified key.
    pub fn create(&mut self, key: &[u8]) -> Result<RandomXVMInstance> {
        if let Some(entry) = self.vms.get_mut(key) {
            let vm = entry.1.clone();
            entry.0 = Instant::now();
            return Ok(vm)
        }

        if self.vms.len() >= self.max_vms {
            if let Some(oldest_key) =
                self.vms.iter().min_by_key(|(_, (i, _))| *i).map(|(k, _)| k.clone())
            {
                self.vms.remove(&oldest_key);
            }
        }

        // Generate a new RandomX virtual machine instance operating in
        // light mode. Memory required per VM in light mode is 256MB.
        let flags = RandomXFlags::get_recommended_flags();
        let (flags, cache) = match RandomXCache::new(flags, key) {
            Ok(cache) => (flags, cache),
            Err(err) => {
                warn!(target: "validator::randomx", "[VALIDATOR] Error initializing RandomX cache with flags {flags:?}: {err}");
                warn!(target: "validator::randomx", "[VALIDATOR] Falling back to default flags");
                let flags = RandomXFlags::DEFAULT;
                let cache = RandomXCache::new(flags, key)?;
                (flags, cache)
            }
        };

        let vm = Arc::new(RandomXVM::new(flags, Some(cache), None)?);
        debug!(target: "validator::randomx", "[VALIDATOR] RandomX VM started with flags = {flags:?}");

        self.vms.insert(Vec::from(key), (Instant::now(), vm.clone()));
        Ok(vm)
    }

    /// Auxiliary function to get the number of VMs currently
    /// allocated.
    pub fn get_count(&self) -> usize {
        self.vms.len()
    }
}
