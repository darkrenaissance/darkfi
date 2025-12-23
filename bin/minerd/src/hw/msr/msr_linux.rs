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

//! Linux-specific MSR implementation using `/dev/cpu/*/msr`

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    os::fd::AsRawFd,
    process::Command,
};

use tracing::warn;

use super::{msr_item::NO_MASK, MsrError, MsrItem, MsrResult};

pub struct MsrImpl {
    available: bool,
    units: Vec<i32>,
}

impl MsrImpl {
    /// Create a new Linux MSR interface
    pub fn new(units: Vec<i32>) -> Self {
        let available = Self::msr_allow_writes() || Self::msr_modprobe();

        if !available {
            warn!("[msr] MSR kernel module not available");
        }

        Self { available, units }
    }

    /// Check if MSR operations are available
    pub fn is_available(&self) -> bool {
        self.available
    }

    /// Get the CPU units
    pub fn units(&self) -> &[i32] {
        &self.units
    }

    /// Execute a callback for each CPU unit
    pub fn write_all<F>(&self, mut callback: F) -> bool
    where
        F: FnMut(i32) -> bool,
    {
        for &cpu in &self.units {
            if !callback(cpu) {
                return false;
            }
        }
        true
    }

    /// Read an MSR register
    pub fn rdmsr(&self, reg: u32, cpu: i32) -> MsrResult<u64> {
        let cpu_id = self.resolve_cpu(cpu)?;
        let path = format!("/dev/cpu/{}/msr", cpu_id);

        let file =
            File::open(&path).map_err(|e| MsrError::ReadError { reg, cpu: cpu_id, source: e })?;

        let mut value = 0u64;
        let bytes_read = unsafe {
            libc::pread(
                file.as_raw_fd(),
                &mut value as *mut u64 as *mut libc::c_void,
                std::mem::size_of::<u64>(),
                reg as libc::off_t,
            )
        };

        if bytes_read == std::mem::size_of::<u64>() as isize {
            Ok(value)
        } else {
            Err(MsrError::ReadError { reg, cpu: cpu_id, source: io::Error::last_os_error() })
        }
    }

    /// Write an MSR register
    pub fn wrmsr(&self, reg: u32, value: u64, cpu: i32) -> MsrResult<()> {
        let cpu_id = self.resolve_cpu(cpu)?;
        let path = format!("/dev/cpu/{}/msr", cpu_id);

        let file = OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|e| MsrError::WriteError { reg, cpu: cpu_id, source: e })?;

        let bytes_written = unsafe {
            libc::pwrite(
                file.as_raw_fd(),
                &value as *const u64 as *const libc::c_void,
                std::mem::size_of::<u64>(),
                reg as libc::off_t,
            )
        };

        if bytes_written == std::mem::size_of::<u64>() as isize {
            Ok(())
        } else {
            Err(MsrError::WriteError { reg, cpu: cpu_id, source: io::Error::last_os_error() })
        }
    }

    /// Write MSR with mask support (read-modify-write)
    pub fn write(&self, reg: u32, value: u64, cpu: i32, mask: u64, verbose: bool) -> bool {
        let write_value = if mask != NO_MASK {
            match self.rdmsr(reg, cpu) {
                Ok(old_value) => MsrItem::masked_value(old_value, value, mask),
                Err(e) => {
                    if verbose {
                        warn!("[msr] Cannot read MSR 0x{:08x}: {}", reg, e);
                    }
                    return false
                }
            }
        } else {
            value
        };

        match self.wrmsr(reg, write_value, cpu) {
            Ok(()) => true,
            Err(e) => {
                if verbose {
                    warn!("[msr] Cannot set MSR 0x{:08x} to 0x{:016x}: {}", reg, write_value, e);
                }
                false
            }
        }
    }

    /// Write an MsrItem
    pub fn write_item(&self, item: &MsrItem, cpu: i32, verbose: bool) -> bool {
        self.write(item.reg(), item.value(), cpu, item.mask(), verbose)
    }

    /// Read MSR and return as MsrItem
    pub fn read(&self, reg: u32, cpu: i32, verbose: bool) -> Option<MsrItem> {
        match self.rdmsr(reg, cpu) {
            Ok(value) => Some(MsrItem::new(reg, value)),
            Err(e) => {
                if verbose {
                    warn!("[msr] Cannot read MSR 0x{:08x}: {}", reg, e);
                }
                None
            }
        }
    }

    /// Resolve CPU index (-1 means use first available)
    fn resolve_cpu(&self, cpu: i32) -> MsrResult<i32> {
        if cpu < 0 {
            self.units.first().copied().ok_or(MsrError::NoCpuUnits)
        } else {
            Ok(cpu)
        }
    }

    /// Try to enable MSR writes via sysfs
    fn msr_allow_writes() -> bool {
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .open("/sys/module/msr/parameters/allow_writes")
            .and_then(|mut file| file.write_all(b"on"))
            .is_ok()
    }

    /// Try to load MSR module via modprobe
    fn msr_modprobe() -> bool {
        Command::new("/sbin/modprobe")
            .args(["msr", "allow_writes=on"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}
