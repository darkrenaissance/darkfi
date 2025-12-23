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

use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuThread {
    affinity: i32,
    intensity: u32,
}

impl CpuThread {
    pub fn new(affinity: i32, intensity: Option<u32>) -> Self {
        Self { affinity, intensity: intensity.unwrap_or(0) }
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        self.intensity <= 8
    }

    #[inline]
    pub fn affinity(&self) -> i32 {
        self.affinity
    }

    #[inline]
    pub fn intensity(&self) -> u32 {
        if self.intensity == 0 {
            1
        } else {
            self.intensity
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuThreads {
    affinity: i32,
    data: Vec<CpuThread>,
}

impl CpuThreads {
    pub fn new(count: usize, intensity: u32) -> Self {
        let mut self_ = Self { affinity: -1, data: Vec::with_capacity(count) };

        for _ in 0..count {
            self_.add(CpuThread::new(-1, Some(intensity)));
        }

        self_
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn add(&mut self, thread: CpuThread) {
        self.data.push(thread)
    }

    pub fn threads(&self) -> &[CpuThread] {
        &self.data
    }
}

#[inline]
pub fn get_affinity(index: u64, affinity: i32) -> i32 {
    if affinity < 0 {
        return -1
    }

    let affinity = affinity as u64;
    let mut idx = 0u64;

    for i in 0..64 {
        if (affinity & (1u64 << i)) == 0 {
            continue
        }

        if idx == index {
            return i
        }

        idx += 1;
    }

    -1
}

/// Binds the current thread to the specified core(s)
pub fn set_thread_affinity<B: AsRef<[usize]>>(core_ids: B) -> io::Result<()> {
    os::set_thread_affinity(core_ids.as_ref())
}

/// Returns a list of cores that the current thread is bound to
pub fn get_thread_affinity() -> io::Result<Vec<usize>> {
    os::get_thread_affinity()
}

// https://github.com/elast0ny/affinity/
// Licensed under MIT
#[cfg(target_os = "linux")]
mod os {
    use libc::{
        cpu_set_t, pid_t, sched_getaffinity, sched_setaffinity, CPU_ISSET, CPU_SET, CPU_SETSIZE,
    };
    use std::{
        io,
        mem::{size_of, zeroed},
    };

    pub(super) fn set_thread_affinity(core_ids: &[usize]) -> io::Result<()> {
        let mut set: cpu_set_t = unsafe { zeroed() };
        unsafe {
            for core_id in core_ids {
                CPU_SET(*core_id, &mut set);
            }
        }

        _sched_setaffinity(0, size_of::<cpu_set_t>(), &set)
    }

    pub(super) fn get_thread_affinity() -> io::Result<Vec<usize>> {
        let mut affinity = vec![];
        let mut set: cpu_set_t = unsafe { zeroed() };

        _sched_getaffinity(0, size_of::<cpu_set_t>(), &mut set)?;

        for i in 0..CPU_SETSIZE as usize {
            if unsafe { CPU_ISSET(i, &set) } {
                affinity.push(i);
            }
        }

        Ok(affinity)
    }

    fn _sched_setaffinity(pid: pid_t, cpusetsize: usize, mask: &cpu_set_t) -> io::Result<()> {
        let res = unsafe { sched_setaffinity(pid, cpusetsize, mask) };
        if res != 0 {
            return Err(io::Error::last_os_error())
        }
        Ok(())
    }

    fn _sched_getaffinity(pid: pid_t, cpusetsize: usize, mask: &mut cpu_set_t) -> io::Result<()> {
        let res = unsafe { sched_getaffinity(pid, cpusetsize, mask) };
        if res != 0 {
            return Err(io::Error::last_os_error())
        }
        Ok(())
    }
}
