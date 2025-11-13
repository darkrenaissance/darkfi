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

use randomx::{RandomXCache, RandomXDataset, RandomXFlags, RandomXVM};
use tracing::{debug, warn};

use crate::{
    system::thread_priority::{set_thread_priority, ThreadPriority},
    Result,
};

/// Wrapper for creating a [`RandomXDataset`]
pub fn init_dataset_wrapper(
    flags: RandomXFlags,
    cache: RandomXCache,
    start_item: u32,
    item_count: u32,
    priority: ThreadPriority,
) -> Result<RandomXDataset> {
    set_thread_priority(priority);
    Ok(RandomXDataset::new(flags, cache, start_item, item_count)?)
}

/// The RandomX virtual machine instance used to verify mining.
#[derive(Clone)]
pub struct RandomXVMInstance {
    // Note: If a cache and dataset (if assigned) allocated to the VM drops,
    // the VM will crash. The cache and dataset for the VM need to be stored
    // together with it since they are not mix and match.
    instance: Arc<RwLock<RandomXVM>>,
}

impl RandomXVMInstance {
    fn create(
        key: &[u8],
        flags: RandomXFlags,
        cache: Option<RandomXCache>,
        dataset: Option<RandomXDataset>,
    ) -> Result<Self> {
        // Note: Memory required per VM in light mode is 256MB
        // RandomXFlags::FULLMEM and RandomXFlags::LARGEPAGES are incompatible
        // with light mode. These are not set by RandomX automatically even in
        // fast mode.
        let (flags, cache) = match cache {
            Some(c) => (flags, c),
            None => match RandomXCache::new(flags, key) {
                Ok(cache) => (flags, cache),
                Err(err) => {
                    warn!(
                        target: "validator::randomx",
                        "[VALIDATOR] Error initializing RandomX cache with flags {:?}: {}",
                        flags, err,
                    );
                    warn!(
                        target: "validator::randomx",
                        "[VALIDATOR] Falling back to default flags",
                    );
                    let flags = RandomXFlags::DEFAULT;
                    let cache = RandomXCache::new(flags, key)?;
                    (flags, cache)
                }
            },
        };

        let vm = RandomXVM::new(flags, Some(cache), dataset)?;

        Ok(Self { instance: Arc::new(RwLock::new(vm)) })
    }

    /// Calculate the RandomX mining hash
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
    /// Create a new RandomXFactory with the specified maximum number of VMs
    pub fn new(max_vms: usize) -> Self {
        Self { inner: Arc::new(RwLock::new(RandomXFactoryInner::new(max_vms))) }
    }

    /// Create a new RandomXFactory with the specified maximum number of VMs
    /// and given RandomXFlags
    pub fn new_with_flags(max_vms: usize, flags: RandomXFlags) -> Self {
        Self { inner: Arc::new(RwLock::new(RandomXFactoryInner::new_with_flags(max_vms, flags))) }
    }

    /// Create a new RandomX VM instance with the specified key
    pub fn create(
        &self,
        key: &[u8],
        cache: Option<RandomXCache>,
        dataset: Option<RandomXDataset>,
    ) -> Result<RandomXVMInstance> {
        let res;
        {
            let mut inner = self.inner.write().unwrap();
            res = inner.create(key, cache, dataset)?;
        }
        Ok(res)
    }

    /// Get the number of VMs currently allocated
    pub fn get_count(&self) -> Result<usize> {
        let inner = self.inner.read().unwrap();
        Ok(inner.get_count())
    }

    /// Get the flags used to create the VMs
    pub fn get_flags(&self) -> Result<RandomXFlags> {
        let inner = self.inner.read().unwrap();
        Ok(inner.get_flags())
    }
}

struct RandomXFactoryInner {
    flags: RandomXFlags,
    vms: HashMap<Vec<u8>, (Instant, RandomXVMInstance)>,
    max_vms: usize,
}

impl RandomXFactoryInner {
    /// Create a new RandomXFactoryInner
    pub(crate) fn new(max_vms: usize) -> Self {
        let flags = RandomXFlags::get_recommended_flags();
        debug!(
            target: "validator::randomx",
            "RandomXFactory started with {} max VMs and recommended flags = {:?}",
            max_vms, flags,
        );

        Self { flags, vms: Default::default(), max_vms }
    }

    pub(crate) fn new_with_flags(max_vms: usize, flags: RandomXFlags) -> Self {
        debug!(
            target: "validator::randomx",
            "RandomX Factory started with {} max VMs and flags = {:?}",
            max_vms, flags,
        );

        Self { flags, vms: Default::default(), max_vms }
    }

    /// Create a new RandomXVMInstance
    pub(crate) fn create(
        &mut self,
        key: &[u8],
        cache: Option<RandomXCache>,
        dataset: Option<RandomXDataset>,
    ) -> Result<RandomXVMInstance> {
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

        let vm = RandomXVMInstance::create(key, self.flags, cache, dataset)?;
        self.vms.insert(Vec::from(key), (Instant::now(), vm.clone()));
        Ok(vm)
    }

    /// Get the number of VMs currently allocated
    pub(crate) fn get_count(&self) -> usize {
        self.vms.len()
    }

    /// Get the flags used to create the VMs
    pub(crate) fn get_flags(&self) -> RandomXFlags {
        self.flags
    }
}

impl fmt::Debug for RandomXFactoryInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RandomXFactory")
            .field("flags", &self.flags)
            .field("max_vms", &self.max_vms)
            .finish()
    }
}
