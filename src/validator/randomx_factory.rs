/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, RwLock},
    time::Instant,
};

use randomx::{RandomXCache, RandomXFlags, RandomXVM};
use tracing::{debug, warn};

use crate::Result;

/// The RandomX light mode virtual machine instance used to verify
/// mining.
#[derive(Clone)]
pub struct RandomXVMInstance {
    instance: Arc<RwLock<RandomXVM>>,
}

impl RandomXVMInstance {
    /// Generate a new RandomX virtual machine instance operating in
    /// light mode. Memory required per VM in light mode is 256MB.
    fn create(key: &[u8]) -> Result<Self> {
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

        let vm = RandomXVM::new(flags, Some(cache), None)?;
        debug!(target: "validator::randomx", "[VALIDATOR] RandomX VM started with flags = {flags:?}");

        Ok(Self { instance: Arc::new(RwLock::new(vm)) })
    }

    /// Calculate the RandomX mining hash.
    pub fn calculate_hash(&self, input: &[u8]) -> Result<Vec<u8>> {
        let lock = self.instance.write().unwrap();
        Ok(lock.calculate_hash(input)?)
    }
}

unsafe impl Send for RandomXVMInstance {}
unsafe impl Sync for RandomXVMInstance {}

/// The RandomX factory that manages the creation of RandomX VMs.
#[derive(Clone, Debug)]
pub struct RandomXFactory {
    /// Threadsafe impl of the inner impl
    inner: Arc<RwLock<RandomXFactoryInner>>,
}

impl Default for RandomXFactory {
    fn default() -> Self {
        Self::new(2)
    }
}

impl RandomXFactory {
    /// Create a new RandomXFactory with the specified maximum number of VMs.
    pub fn new(max_vms: usize) -> Self {
        Self { inner: Arc::new(RwLock::new(RandomXFactoryInner::new(max_vms))) }
    }

    /// Create a new RandomX VM instance with the specified key.
    pub fn create(&self, key: &[u8]) -> Result<RandomXVMInstance> {
        let res;
        {
            let mut inner = self.inner.write().unwrap();
            res = inner.create(key)?;
        }
        Ok(res)
    }

    /// Auxiliary function to get the number of VMs currently
    /// allocated.
    pub fn get_count(&self) -> Result<usize> {
        let inner = self.inner.read().unwrap();
        Ok(inner.get_count())
    }
}

struct RandomXFactoryInner {
    vms: HashMap<Vec<u8>, (Instant, RandomXVMInstance)>,
    max_vms: usize,
}

impl RandomXFactoryInner {
    /// Create a new RandomXFactoryInner.
    pub(crate) fn new(max_vms: usize) -> Self {
        debug!(target: "validator::randomx", "[VALIDATOR] RandomXFactory started with {max_vms} max VMs");
        Self { vms: Default::default(), max_vms }
    }

    /// Create a new RandomXVMInstance.
    pub(crate) fn create(&mut self, key: &[u8]) -> Result<RandomXVMInstance> {
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

        let vm = RandomXVMInstance::create(key)?;
        self.vms.insert(Vec::from(key), (Instant::now(), vm.clone()));
        Ok(vm)
    }

    /// Get the number of VMs currently allocated
    pub(crate) fn get_count(&self) -> usize {
        self.vms.len()
    }
}

impl fmt::Debug for RandomXFactoryInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RandomXFactory").field("max_vms", &self.max_vms).finish()
    }
}
