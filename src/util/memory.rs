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

//! Process memory telemetry helpers.

use std::fmt;

use super::logger::verbose;

/// Best-effort process memory snapshot for the current platform.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MemorySnapshot {
    /// Current resident set size in bytes.
    pub rss_bytes: Option<u64>,
    /// Peak resident set size in bytes.
    pub peak_rss_bytes: Option<u64>,
    /// Apple physical footprint in bytes, when available.
    pub physical_footprint_bytes: Option<u64>,
}

/// Return current process memory counters without allocating large state.
pub fn memory_snapshot() -> MemorySnapshot {
    platform_memory_snapshot()
}

/// Log current process memory counters with a stable tag and label.
pub fn log_memory(label: &str) {
    let snapshot = memory_snapshot();
    verbose!(
        target: "darkfi::memory",
        "[MEMORY] {label}: rss={} peak_rss={} physical_footprint={}",
        Bytes(snapshot.rss_bytes),
        Bytes(snapshot.peak_rss_bytes),
        Bytes(snapshot.physical_footprint_bytes),
    );
}

struct Bytes(Option<u64>);

impl fmt::Display for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(bytes) = self.0 else { return f.write_str("unknown") };

        let mib = bytes as f64 / 1_048_576.0;
        write!(f, "{mib:.1} MiB ({bytes} bytes)")
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn platform_memory_snapshot() -> MemorySnapshot {
    let (rss_bytes, peak_rss_bytes) = proc_status_memory();
    MemorySnapshot {
        rss_bytes,
        peak_rss_bytes: peak_rss_bytes.or_else(rusage_peak_rss_bytes),
        physical_footprint_bytes: None,
    }
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_memory_snapshot() -> MemorySnapshot {
    let (rss_bytes, physical_footprint_bytes) = match apple_rusage_info() {
        Some(usage) => (Some(usage.ri_resident_size), Some(usage.ri_phys_footprint)),
        None => (None, None),
    };

    MemorySnapshot { rss_bytes, peak_rss_bytes: rusage_peak_rss_bytes(), physical_footprint_bytes }
}

#[cfg(all(
    unix,
    not(any(target_os = "linux", target_os = "android", target_os = "ios", target_os = "macos"))
))]
fn platform_memory_snapshot() -> MemorySnapshot {
    MemorySnapshot {
        rss_bytes: None,
        peak_rss_bytes: rusage_peak_rss_bytes(),
        physical_footprint_bytes: None,
    }
}

#[cfg(not(unix))]
fn platform_memory_snapshot() -> MemorySnapshot {
    MemorySnapshot::default()
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn proc_status_memory() -> (Option<u64>, Option<u64>) {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else { return (None, None) };

    (parse_status_kib(&status, "VmRSS:"), parse_status_kib(&status, "VmHWM:"))
}

#[cfg(any(target_os = "linux", target_os = "android", test))]
fn parse_status_kib(status: &str, key: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let value = line.strip_prefix(key)?.split_whitespace().next()?;
        value.parse::<u64>().ok().map(|kib| kib.saturating_mul(1024))
    })
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn apple_rusage_info() -> Option<libc::rusage_info_v4> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage_info_v4>::uninit();
    let ret = unsafe {
        libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V4,
            usage.as_mut_ptr() as *mut libc::rusage_info_t,
        )
    };

    if ret == 0 {
        Some(unsafe { usage.assume_init() })
    } else {
        None
    }
}

#[cfg(unix)]
fn rusage_peak_rss_bytes() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if ret != 0 {
        return None
    }

    let usage = unsafe { usage.assume_init() };
    u64::try_from(usage.ru_maxrss).ok().map(|rss| rss.saturating_mul(rusage_maxrss_multiplier()))
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn rusage_maxrss_multiplier() -> u64 {
    1
}

#[cfg(all(unix, not(any(target_os = "ios", target_os = "macos"))))]
fn rusage_maxrss_multiplier() -> u64 {
    1024
}
