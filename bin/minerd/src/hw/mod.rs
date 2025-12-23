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

use std::collections::HashSet;

use tracing::{debug, error, info, warn};

/// CPU detection pub mod cpuid;
pub mod cpuid;
use cpuid::{CpuInfo, CpuThreads};

/// Model-Specific Registers
pub mod msr;
use msr::{msr_presets, Msr, MsrItem, MsrPreset, NO_MASK};

// MSR registers for Cache QoS
const IA32_PQR_ASSOC: u32 = 0xC8F; // PQR (Platform QoS Resource) Association
const IA32_L3_QOS_MASK_1: u32 = 0xC91; // L3 Cache QoS Mask for COS 1

// Class of Service assignments
const COS_FULL_CACHE: u64 = 0; // COS 0 = full L3 cache access
const COS_LIMITED_CACHE: u64 = 1 << 32; // COS 1 = limited L3 cache (bit 32 sets COS in PQR_ASSOC)

#[derive(Default)]
pub struct RxMsr {
    is_initialized: bool,
    is_enabled: bool,
    cache_qos: bool,
    saved_items: Vec<MsrItem>,
}

impl RxMsr {
    pub fn new() -> Self {
        Self { is_initialized: false, is_enabled: false, cache_qos: false, saved_items: vec![] }
    }

    pub fn init(&mut self, cache_qos: bool, threads: usize, save: bool) -> bool {
        if self.is_initialized {
            return self.is_enabled
        }

        self.is_initialized = true;
        self.is_enabled = false;

        // Detect CPU and find MSR preset
        let cpu = CpuInfo::detect();
        let msr_preset = if cpu.is_amd() && cpu.zen_generation.is_some() {
            msr_presets::get_preset(MsrPreset::from_zen(cpu.zen_generation.unwrap()))
        } else if cpu.is_intel() {
            msr_presets::get_preset(MsrPreset::Intel)
        } else {
            msr_presets::get_preset(MsrPreset::None)
        };

        if msr_preset.is_empty() {
            return false
        }

        self.cache_qos = cache_qos;
        if self.cache_qos && !cpu.has_cat_l3 {
            warn!("This CPU doesn't support cat_l3");
            self.cache_qos = false;
        }

        self.is_enabled = self.wrmsr(msr_preset, threads, self.cache_qos, save);
        if self.is_enabled {
            info!("[msr] MSR register values set successfully");
        } else {
            error!("[msr] Failed to apply MSR mod, hashrate will be low");
        }

        self.is_enabled
    }

    pub fn destroy(&mut self) {
        if !self.is_initialized {
            return
        }

        self.is_initialized = false;
        self.is_enabled = false;

        if self.saved_items.is_empty() {
            return
        }

        let saved_items = std::mem::take(&mut self.saved_items);

        if !self.wrmsr(&saved_items, 0, self.cache_qos, false) {
            error!("[msr] Failed to restore to initial state");
        }
    }

    fn wrmsr(
        &mut self,
        preset: &[MsrItem],
        threads: usize, // TODO: This should be a slice of threads
        cache_qos: bool,
        save: bool,
    ) -> bool {
        let msr = Msr::get();
        if msr.is_none() {
            return false
        }
        let msr = msr.unwrap();

        if save {
            self.saved_items.reserve(preset.len());
            for i in preset {
                if let Some(item) = msr.read(i.reg(), -1, true) {
                    if !item.is_valid() {
                        self.saved_items.clear();
                        return false
                    }

                    self.saved_items.push(item);
                }
            }
        }

        // Which CPU cores will have access top the full L3 cache
        // TODO: Check xmrig/crypto/rx/RxMsr.cpp::wsmsr()
        //let cpu_threads = CpuThreads::detect();
        //let units: HashSet<i32> = cpu_threads.thread_ids().into_iter().collect();

        let cache_enabled: HashSet<i32> = HashSet::new();
        //let mut cache_qos_disabled = threads.is_empty();
        let cache_qos_disabled = true;

        /*
        if cache_qos && !cache_qos_disabled {
            for thread in threads {
                let affinity = thread.affinity();
                // If some thread has no affinity or wrong affinity,
                // disable cache QoS
                if affinity < 0 || !units.contains(&affinity) {
                    cache_qos_disabled = true;
                    warn!(
                        "Cache QoS can only be enabled when all mining threads have affinity set"
                    );
                }
                break
            }
            cache_enabled.insert(affinity);
        }
        */

        // Apply MSR values to all CPUs
        msr.write_all(|cpu| {
            debug!("msr.write_all cpu={} get_cpu={}", cpu, get_cpu(cpu));
            for item in preset {
                if !msr.write(item.reg(), item.value(), get_cpu(cpu), item.mask(), true) {
                    return false
                }
            }

            if !cache_qos {
                return true
            }

            // Cache QoS configuration
            if cache_qos_disabled || cache_enabled.contains(&cpu) {
                // Assign Class of Service 0 (full L3 cache) to this CPU
                return msr.write(IA32_PQR_ASSOC, COS_FULL_CACHE, get_cpu(cpu), NO_MASK, true)
            }

            // For CPUs not running mining threads:
            // Disable L3 cache for Class of Service 1
            if !msr.write(IA32_L3_QOS_MASK_1, 0, get_cpu(cpu), NO_MASK, true) {
                // Some CPUs don't allow setting it to all zeros
                if !msr.write(IA32_L3_QOS_MASK_1, 1, get_cpu(cpu), NO_MASK, true) {
                    return false
                }
            }

            // Assign Class of Service 1 (limited cache) to this CPU
            msr.write(IA32_PQR_ASSOC, COS_LIMITED_CACHE, get_cpu(cpu), NO_MASK, true)
        })
    }
}

#[cfg(target_os = "windows")]
const fn get_cpu(cpu: i32) -> i32 {
    -1
}

#[cfg(not(target_os = "windows"))]
const fn get_cpu(cpu: i32) -> i32 {
    cpu
}
