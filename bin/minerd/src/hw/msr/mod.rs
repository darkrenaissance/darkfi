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

//! Cross-platform module for reading and writing CPU Model Specific Registers
//!
//! ## Platform support
//!
//! - Linux: Uses `/dev/cpu*/msr` interface with automatic module loading
//! - Windows: Uses WinRing0 driver with automatic installation
//!
//! ## Requirements
//!
//! - Linux: root privileges and the `msr` kernel module
//! - Windows: Administrator privileges and `WinRing0x64.sys`

use std::sync::{Arc, Weak};

use once_cell::sync::Lazy;
use parking_lot::Mutex;

mod error;
mod msr_item;
mod msr_presets;

use error::{MsrError, MsrResult};
use msr_item::MsrItem;

#[cfg(target_os = "linux")]
mod msr_linux;

/*
#[cfg(target_os = "windows")]
mod msr_win;
*/

#[cfg(target_os = "linux")]
use msr_linux::MsrImpl;

#[cfg(not(any(target_os = "linux")))]
use unsupported::MsrImpl;

/// Global weak reference to the MSR singleton
static INSTANCE: Lazy<Mutex<Weak<Msr>>> = Lazy::new(|| Mutex::new(Weak::new()));

/// MSR Interface
pub struct Msr {
    inner: MsrImpl,
}

impl Msr {
    /// Get or create the MSR singleton
    ///
    /// Returns `None` if MSR is not available on this system.
    pub fn get(units: Vec<i32>) -> Option<Arc<Self>> {
        let mut instance = INSTANCE.lock();

        // Try to upgrade the weak reference
        if let Some(msr) = instance.upgrade() {
            if msr.is_available() {
                return Some(msr);
            }
        }

        // Create new instance
        let msr = Arc::new(Self { inner: MsrImpl::new(units) });

        if msr.is_available() {
            *instance = Arc::downgrade(&msr);
            Some(msr)
        } else {
            None
        }
    }

    /// Create a new MSR instance without using the singleton.
    ///
    /// Useful for testing or when multiple independent instances are needed.
    pub fn new_instance(units: Vec<i32>) -> Self {
        Self { inner: MsrImpl::new(units) }
    }

    /// Check if MSR operations are available
    pub fn is_available(&self) -> bool {
        self.inner.is_available()
    }

    /// Get the CPU units this MSR operates on
    pub fn units(&self) -> &[i32] {
        self.inner.units()
    }

    /// Write an MsrItem to a CPU
    ///
    /// * `item`: The MSR item to write
    /// * `cpu`: CPU index (-1 for default/first)
    /// * `verbose`: Log warnings on failure
    pub fn write_item(&self, item: &MsrItem, cpu: i32, verbose: bool) -> bool {
        self.inner.write_item(item, cpu, verbose)
    }

    /// Write to an MSR register with optional mask
    ///
    /// If a mask is provided, this performs a read-modify-write
    ///
    /// * `reg`: MSR register address
    /// * `value`: Value to write
    /// * `cpu`: CPU index (-1 for default/first)
    /// * `mask`: Bit mask for partial updates (use NO_MASK for full write)
    /// * `verbose`: Log warnings on failure
    pub fn write(&self, reg: u32, value: u64, cpu: i32, mask: u64, verbose: bool) -> bool {
        self.inner.write(reg, value, cpu, mask, verbose)
    }

    /// Execute a callback for each CPU unit
    ///
    /// The callback receives the CPU ID and should return `true` to continue
    /// or `false` to abort.
    pub fn write_all<F>(&self, callback: F) -> bool
    where
        F: FnMut(i32) -> bool + Send,
    {
        self.inner.write_all(callback)
    }

    /// Read an MSR register and return as MsrItem
    ///
    /// * `reg`: MSR register address
    /// * `cpu`: CPU index (-1 for default/first)
    /// * `verbose`: Log warnings on failure
    pub fn read(&self, reg: u32, cpu: i32, verbose: bool) -> Option<MsrItem> {
        self.inner.read(reg, cpu, verbose)
    }

    /// Low-level MSR read
    ///
    /// Returns the raw value or error
    pub fn rdmsr(&self, reg: u32, cpu: i32) -> MsrResult<u64> {
        self.inner.rdmsr(reg, cpu)
    }

    /// Low-level MSR write
    ///
    /// Writes the value directly without masking
    pub fn wrmsr(&self, reg: u32, value: u64, cpu: i32) -> MsrResult<()> {
        self.inner.wrmsr(reg, value, cpu)
    }
}

/// Impls for unsupported platforms.
#[cfg(not(any(target_os = "linux")))]
mod unsupported {
    use super::*;
    use tracing::warn;

    pub struct MsrImpl {
        units: Vec<i32>,
    }

    impl MsrImpl {
        pub fn new(units: Vec<i32>) -> Self {
            warn!("[msr] MSR not supported on this platform");
            Self { units }
        }

        pub fn is_available(&self) -> bool {
            false
        }

        pub fn units(&self) -> &[i32] {
            &self.units
        }

        pub fn write_all<F>(&self, _callback: F) -> bool
        where
            F: FnMut(i32) -> bool,
        {
            false
        }

        pub fn rdmsr(&self, reg: u32, cpu: i32) -> MsrResult<u64> {
            Err(crate::error::MsrError::PlatformNotSupported)
        }

        pub fn wrmsr(&self, _reg: u32, _value: u64, _cpu: i32) -> MsrResult<()> {
            Err(crate::error::MsrError::PlatformNotSupported)
        }

        pub fn write(&self, _reg: u32, _value: u64, _cpu: i32, _mask: u64, _verbose: bool) -> bool {
            false
        }

        pub fn write_item(&self, _item: &MsrItem, _cpu: i32, _verbose: bool) -> bool {
            false
        }

        pub fn read(&self, _reg: u32, _cpu: i32, _verbose: bool) -> Option<MsrItem> {
            None
        }
    }
}
