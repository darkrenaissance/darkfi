/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use parking_lot::Mutex as SyncMutex;

use super::{EpochIndex, Renderer};

/// Cache for draw outputs holding epoch-scoped GPU resources (buffers,
/// textures, anims). Such resources only exist for the current UI epoch;
/// when the epoch is bumped (e.g. the GL context is recreated on Android)
/// every resource from the dead epoch is gone, so stale entries must
/// never be served.
pub struct EpochCache<T> {
    /// Shared with the `Renderer`; bumped on UI restart
    epoch: Arc<AtomicU32>,
    inner: SyncMutex<Option<(EpochIndex, T)>>,
}

impl<T: Clone> EpochCache<T> {
    pub fn new(renderer: &Renderer) -> Self {
        Self { epoch: renderer.epoch.clone(), inner: SyncMutex::new(None) }
    }

    /// Returns the cached value, or None when empty, cleared, or built
    /// under a dead epoch.
    pub fn get(&self) -> Option<T> {
        let mut cache = self.inner.lock();
        // A stale entry belongs to a dead epoch and can never become
        // valid again, so evict it here instead of holding it in memory
        // until the next set().
        match cache.take() {
            Some((e, v)) if e == self.epoch.load(Ordering::Relaxed) => {
                *cache = Some((e, v.clone()));
                Some(v)
            }
            _ => None,
        }
    }

    /// Returns the cached value, computing and storing it via `f` when
    /// absent, cleared, or stale. `f` runs under the lock so a concurrent
    /// `clear()` cannot be lost between the read and the store.
    pub fn get_or_insert_with(&self, f: impl FnOnce() -> T) -> T {
        let mut cache = self.inner.lock();
        let cur = self.epoch.load(Ordering::Relaxed);
        if let Some((e, v)) = &*cache {
            if *e == cur {
                return v.clone()
            }
        }
        let v = f();
        *cache = Some((cur, v.clone()));
        v
    }

    pub fn set(&self, value: T) {
        let e = self.epoch.load(Ordering::Relaxed);
        *self.inner.lock() = Some((e, value));
    }

    pub fn clear(&self) {
        *self.inner.lock() = None;
    }
}

/// Remembers the UI epoch last seen and reports when it has changed, e.g.
/// to drop caches holding epoch-scoped GPU resources.
pub struct EpochTracker {
    epoch: Arc<AtomicU32>,
    cached: EpochIndex,
}

impl EpochTracker {
    pub fn new(renderer: &Renderer) -> Self {
        Self { epoch: renderer.epoch.clone(), cached: renderer.epoch.load(Ordering::Relaxed) }
    }

    /// Returns true when the epoch changed since the last call
    pub fn changed(&mut self) -> bool {
        let cur = self.epoch.load(Ordering::Relaxed);
        if cur == self.cached {
            return false
        }

        self.cached = cur;
        true
    }
}
