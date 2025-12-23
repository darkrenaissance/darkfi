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

use std::{collections::HashMap, fmt};

/// CPU Vendor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Intel,
    Amd,
    Unknown,
}

impl fmt::Display for Vendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Vendor::Intel => write!(f, "Intel"),
            Vendor::Amd => write!(f, "AMD"),
            Vendor::Unknown => write!(f, "Unknown"),
        }
    }
}

/// AMD CPU microarchitecture generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmdZenGeneration {
    /// Zen 1 (Ryzen 1000, EPYC Naples) - Family 17h, Models 00-0F
    /// Zen+ (Ryzen 2000) - Family 17h, Models 10-1F
    /// Zen 2 (Ryzen 3000, EPYC Rome) - Family 17h, Models 30-3F, 60-6F, 70-7F, 90-9F
    Zen1OrZen2,

    /// Zen 3 (Ryzen 5000, EPYC Milan) - Family 19h (25), various models
    Zen3,

    /// Zen 4 (Ryzen 7000, EPYC Genoa) - Family 19h (25), Models 61h (97), 75h (117)
    Zen4,

    /// Zen 5 (Ryzen 9000) - Family 1Ah (26)
    Zen5,

    /// Unknown or unsupported AMD CPU
    Unknown,
}

impl fmt::Display for AmdZenGeneration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AmdZenGeneration::Zen1OrZen2 => write!(f, "Zen1/Zen2"),
            AmdZenGeneration::Zen3 => write!(f, "Zen3"),
            AmdZenGeneration::Zen4 => write!(f, "Zen4"),
            AmdZenGeneration::Zen5 => write!(f, "Zen5"),
            AmdZenGeneration::Unknown => write!(f, "Unknown AMD"),
        }
    }
}

/// Detected CPU information
#[derive(Debug, Clone)]
pub struct CpuInfo {
    /// CPU vendor
    pub vendor: Vendor,
    /// Vendor string (e.g., "GenuineIntel", "AuthenticAMD")
    pub vendor_string: String,
    /// CPU family (after extended family calculation)
    pub family: u32,
    /// CPU model (after extended model calculation)
    pub model: u32,
    /// CPU stepping
    pub stepping: u32,
    /// Brand string (e.g., "AMD Ryzen 9 7950X")
    pub brand_string: String,
    /// AMD Zen generation (if AMD)
    pub zen_generation: Option<AmdZenGeneration>,
    /// CPU supports CAT L3
    pub has_cat_l3: bool,
}

impl CpuInfo {
    /// Detect the CPU using CPUID instruction
    pub fn detect() -> Self {
        let (vendor_string, vendor) = get_vendor();
        let (family, model, stepping) = get_family_model_stepping();
        let brand_string = get_brand_string();

        let zen_generation =
            if vendor == Vendor::Amd { Some(detect_zen_generation(family, model)) } else { None };

        let has_cat_l3 = detect_cat_l3();

        Self {
            vendor,
            vendor_string,
            family,
            model,
            stepping,
            brand_string,
            zen_generation,
            has_cat_l3,
        }
    }

    pub fn threads(&self) -> CpuThreads {
        CpuThreads::detect()
    }

    /// Check if this is an AMD CPU
    pub fn is_amd(&self) -> bool {
        self.vendor == Vendor::Amd
    }

    /// Check if this is an Intel CPU
    pub fn is_intel(&self) -> bool {
        self.vendor == Vendor::Intel
    }

    /// Check if this is an AMD Ryzen or EPYC CPU
    pub fn is_ryzen_or_epyc(&self) -> bool {
        self.is_amd() && (self.brand_string.contains("Ryzen") || self.brand_string.contains("EPYC"))
    }
}

impl fmt::Display for CpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "CPU Information:")?;
        writeln!(f, "  Vendor: {} ({})", self.vendor, self.vendor_string)?;
        writeln!(f, "  Family: {} (0x{:X})", self.family, self.family)?;
        writeln!(f, "  Model: {} (0x{:X})", self.model, self.model)?;
        writeln!(f, "  Stepping: {}", self.stepping)?;
        if !self.brand_string.is_empty() {
            writeln!(f, "  Brand: {}", self.brand_string)?;
        }
        if let Some(zen) = &self.zen_generation {
            writeln!(f, "  Generation: {}", zen)?;
        }
        writeln!(f, "  CAT L3: {}", if self.has_cat_l3 { "supported" } else { "not supported" })?;
        Ok(())
    }
}

/// Execute CPUID instruction
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn cpuid(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32) {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::{CpuidResult, __cpuid_count};
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::{CpuidResult, __cpuid_count};

    let result: CpuidResult = unsafe { __cpuid_count(leaf, subleaf) };
    (result.eax, result.ebx, result.ecx, result.edx)
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cpuid(_leaf: u32, _subleaf: u32) -> (u32, u32, u32, u32) {
    (0, 0, 0, 0)
}

/// Get CPU vendor string and enum
fn get_vendor() -> (String, Vendor) {
    let (_, ebx, ecx, edx) = cpuid(0, 0);

    // Vendor string is in EBX, EDX, ECX (in that order)
    let vendor_bytes: [u8; 12] = [
        (ebx & 0xFF) as u8,
        ((ebx >> 8) & 0xFF) as u8,
        ((ebx >> 16) & 0xFF) as u8,
        ((ebx >> 24) & 0xFF) as u8,
        (edx & 0xFF) as u8,
        ((edx >> 8) & 0xFF) as u8,
        ((edx >> 16) & 0xFF) as u8,
        ((edx >> 24) & 0xFF) as u8,
        (ecx & 0xFF) as u8,
        ((ecx >> 8) & 0xFF) as u8,
        ((ecx >> 16) & 0xFF) as u8,
        ((ecx >> 24) & 0xFF) as u8,
    ];

    let vendor_string = String::from_utf8_lossy(&vendor_bytes).to_string();

    let vendor = match vendor_string.as_str() {
        "GenuineIntel" => Vendor::Intel,
        "AuthenticAMD" => Vendor::Amd,
        _ => Vendor::Unknown,
    };

    (vendor_string, vendor)
}

/// Get CPU family, model, and stepping from CPUID leaf 1
fn get_family_model_stepping() -> (u32, u32, u32) {
    let (eax, _, _, _) = cpuid(1, 0);

    // Extract base values
    let stepping = eax & 0xF;
    let base_model = (eax >> 4) & 0xF;
    let base_family = (eax >> 8) & 0xF;
    let ext_model = (eax >> 16) & 0xF;
    let ext_family = (eax >> 20) & 0xFF;

    // Calculate actual family and model
    // For family >= 0xF, add extended family
    let family = if base_family == 0xF { base_family + ext_family } else { base_family };

    // For family >= 0xF (AMD) or family == 0x6 (Intel), use extended model
    let model = if base_family >= 0xF || base_family == 0x6 {
        (ext_model << 4) | base_model
    } else {
        base_model
    };

    (family, model, stepping)
}

/// Get CPU brand string from CPUID leaves 0x80000002-0x80000004
fn get_brand_string() -> String {
    // Check if extended CPUID is supported
    let (max_extended, _, _, _) = cpuid(0x80000000, 0);

    if max_extended < 0x80000004 {
        return String::new();
    }

    let mut brand_bytes = [0u8; 48];

    for (i, leaf) in [0x80000002u32, 0x80000003, 0x80000004].iter().enumerate() {
        let (eax, ebx, ecx, edx) = cpuid(*leaf, 0);
        let offset = i * 16;

        brand_bytes[offset..offset + 4].copy_from_slice(&eax.to_le_bytes());
        brand_bytes[offset + 4..offset + 8].copy_from_slice(&ebx.to_le_bytes());
        brand_bytes[offset + 8..offset + 12].copy_from_slice(&ecx.to_le_bytes());
        brand_bytes[offset + 12..offset + 16].copy_from_slice(&edx.to_le_bytes());
    }

    // Convert to string and trim null bytes and whitespace
    String::from_utf8_lossy(&brand_bytes)
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .to_string()
}

/// Detect AMD Zen generation based on family and model
fn detect_zen_generation(family: u32, model: u32) -> AmdZenGeneration {
    match family {
        // Family 17h (23) = Zen, Zen+, Zen 2
        0x17 => AmdZenGeneration::Zen1OrZen2,

        // Family 19h (25) = Zen 3 or Zen 4
        0x19 => {
            // Zen 4 models: 0x61 (97), 0x75 (117)
            // Also: 0x70-0x7F range for some Zen 4 parts
            match model {
                0x61 => AmdZenGeneration::Zen4,        // Raphael
                0x75 => AmdZenGeneration::Zen4,        // Phoenix
                0x10..=0x1F => AmdZenGeneration::Zen4, // Genoa
                0x70..=0x7F => AmdZenGeneration::Zen4, // Dragon Range
                0xA0..=0xAF => AmdZenGeneration::Zen4, // Phoenix 2
                _ => AmdZenGeneration::Zen3,           // Other family 25 = Zen 3
            }
        }

        // Family 1Ah (26) = Zen 5
        0x1A => AmdZenGeneration::Zen5,

        _ => AmdZenGeneration::Unknown,
    }
}

/// Detect Cache Allocation Technology L3 support
fn detect_cat_l3() -> bool {
    let (max_leaf, _, _, _) = cpuid(0, 0);
    if max_leaf < 7 {
        return false;
    }

    let (_, ebx, _, _) = cpuid(7, 0);
    let has_rdt_a = (ebx >> 15) & 1 != 0;

    if !has_rdt_a {
        return false;
    }

    if max_leaf < 0x10 {
        return false;
    }

    let (_, ebx, _, _) = cpuid(0x10, 0);
    (ebx >> 1) & 1 != 0
}

/// Get the number of logical CPUs
pub fn get_cpu_count() -> usize {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir("/sys/devices/system/cpu")
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name();
                        let name = name.to_string_lossy();
                        name.starts_with("cpu") &&
                            name.chars().nth(3).map(|c| c.is_ascii_digit()).unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(1)
    }

    #[cfg(target_os = "windows")]
    {
        std::env::var("NUMBER_OF_PROCESSORS").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        1
    }
}

/// Get list of CPU unit IDs
pub fn get_cpu_units() -> Vec<i32> {
    (0..get_cpu_count() as i32).collect()
}

/// Information about a single CPU/logical processor
#[derive(Debug, Clone)]
pub struct CpuThread {
    /// Logical processor ID (used for affinity)
    pub id: i32,
    /// Physical core ID this thread belongs to
    pub core_id: i32,
    /// Package/socket ID
    pub package_id: i32,
    /// NUMA node ID
    pub node_id: i32,
}

/// CPU topology information
#[derive(Debug, Clone)]
pub struct CpuThreads {
    /// All logical processors
    threads: Vec<CpuThread>,
    /// Number of physical packages/sockets
    packages: u32,
    /// Number of physical cores (total across all packages)
    cores: u32,
    /// Number of logical processors (threads)
    logical: u32,
    /// Number of NUMA nodes
    nodes: u32,
    /// L3 cache size in bytes (per package, 0 if unknown)
    l3_cache_size: u64,
    /// SMT/Hyperthreading support
    smt_enabled: bool,
}

impl CpuThreads {
    pub fn detect() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self::detect_linux()
        }
        #[cfg(target_os = "windows")]
        {
            Self::detect_windows()
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            Self::detect_fallback()
        }
    }

    pub fn threads(&self) -> &[CpuThread] {
        &self.threads
    }

    pub fn thread_ids(&self) -> Vec<i32> {
        self.threads.iter().map(|t| t.id).collect()
    }

    pub fn one_per_core(&self) -> Vec<i32> {
        let mut seen_cores: HashMap<(i32, i32), i32> = HashMap::new();

        for thread in &self.threads {
            let key = (thread.package_id, thread.core_id);
            seen_cores.entry(key).or_insert(thread.id);
        }

        let mut result: Vec<i32> = seen_cores.into_values().collect();
        result.sort();
        result
    }

    pub fn one_per_package(&self) -> Vec<i32> {
        let mut seen_packages: HashMap<i32, i32> = HashMap::new();

        for thread in &self.threads {
            seen_packages.entry(thread.package_id).or_insert(thread.id);
        }

        let mut result: Vec<i32> = seen_packages.into_values().collect();
        result.sort();
        result
    }

    pub fn threads_for_package(&self, package_id: i32) -> Vec<i32> {
        self.threads.iter().filter(|t| t.package_id == package_id).map(|t| t.id).collect()
    }

    pub fn threads_for_node(&self, node_id: i32) -> Vec<i32> {
        self.threads.iter().filter(|t| t.node_id == node_id).map(|t| t.id).collect()
    }

    pub fn packages(&self) -> u32 {
        self.packages
    }

    pub fn cores(&self) -> u32 {
        self.cores
    }

    pub fn logical(&self) -> u32 {
        self.logical
    }

    pub fn nodes(&self) -> u32 {
        self.nodes
    }

    pub fn l3_cache_size(&self) -> u64 {
        self.l3_cache_size
    }

    pub fn smt_enabled(&self) -> bool {
        self.smt_enabled
    }

    pub fn threads_per_core(&self) -> u32 {
        if self.cores > 0 {
            self.logical / self.cores
        } else {
            1
        }
    }

    #[cfg(target_os = "linux")]
    fn detect_linux() -> Self {
        let mut threads = Vec::new();
        let mut max_package = 0i32;
        let mut max_node = 0i32;
        let mut core_set: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
        let mut l3_cache_size = 0u64;

        // Enumerate all CPUs
        if let Ok(entries) = std::fs::read_dir("/sys/devices/system/cpu") {
            let mut cpu_ids: Vec<i32> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    if let Some(n) = name.strip_prefix("cpu") {
                        n.parse::<i32>().ok()
                    } else {
                        None
                    }
                })
                .collect();

            cpu_ids.sort();

            for cpu_id in cpu_ids {
                let base_path = format!("/sys/devices/system/cpu/cpu{}", cpu_id);

                // Check if this CPU is online (cpu0 is always online)
                if cpu_id != 0 {
                    let online_path = format!("{}/online", base_path);
                    if let Ok(online) = std::fs::read_to_string(&online_path) {
                        if online.trim() == "0" {
                            continue; // Skip offline CPUs
                        }
                    }
                }

                let topology_path = format!("{}/topology", base_path);

                let core_id = read_sysfs_int(&format!("{}/core_id", topology_path)).unwrap_or(0);
                let package_id =
                    read_sysfs_int(&format!("{}/physical_package_id", topology_path)).unwrap_or(0);

                // NUMA node detection
                let node_id = detect_numa_node(cpu_id);

                max_package = max_package.max(package_id);
                max_node = max_node.max(node_id);
                core_set.insert((package_id, core_id));

                threads.push(CpuThread { id: cpu_id, core_id, package_id, node_id });

                // Get L3 cache size (once)
                if l3_cache_size == 0 {
                    l3_cache_size = detect_l3_cache_size(cpu_id);
                }
            }
        }

        let logical = threads.len() as u32;
        let cores = core_set.len() as u32;
        let packages = (max_package + 1) as u32;
        let nodes = (max_node + 1) as u32;
        let smt_enabled = logical > cores;

        Self { threads, packages, cores, logical, nodes, l3_cache_size, smt_enabled }
    }

    #[cfg(target_os = "windows")]
    fn detect_windows() -> Self {
        use std::{mem, ptr};

        // We'll use GetLogicalProcessorInformationEx for detailed topology
        // For now, use a simpler approach with environment variables and CPUID

        let logical = std::env::var("NUMBER_OF_PROCESSORS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);

        // Try to get more detailed info via Windows API
        let (packages, cores, nodes, l3_cache_size) = get_windows_topology_info();

        let smt_enabled = logical > cores;

        // Build thread list (simplified - assumes sequential IDs)
        let threads_per_core = if cores > 0 { logical / cores } else { 1 };
        let cores_per_package = if packages > 0 { cores / packages } else { cores };

        let mut threads = Vec::with_capacity(logical as usize);

        for i in 0..logical {
            let core_id = (i / threads_per_core) as i32;
            let package_id = (core_id as u32 / cores_per_package) as i32;

            threads.push(CpuThread {
                id: i as i32,
                core_id: core_id % cores_per_package as i32,
                package_id,
                node_id: package_id, // Simplified: assume 1 NUMA node per package
            });
        }

        Self { threads, packages, cores, logical, nodes, l3_cache_size, smt_enabled }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    fn detect_fallback() -> Self {
        let logical = 1u32;

        Self {
            threads: vec![CpuThread { id: 0, core_id: 0, package_id: 0, node_id: 0 }],
            packages: 1,
            cores: 1,
            logical,
            nodes: 1,
            l3_cache_size: 0,
            smt_enabled: false,
        }
    }
}

impl fmt::Display for CpuThreads {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "CPU Topology:")?;
        writeln!(f, "  Packages: {}", self.packages)?;
        writeln!(f, "  Cores: {}", self.cores)?;
        writeln!(f, "  Threads: {}", self.logical)?;
        writeln!(f, "  NUMA nodes: {}", self.nodes)?;
        writeln!(f, "  SMT: {}", if self.smt_enabled { "enabled" } else { "disabled" })?;
        if self.l3_cache_size > 0 {
            writeln!(f, "  L3 Cache: {} MB", self.l3_cache_size / (1024 * 1024))?;
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn read_sysfs_int(path: &str) -> Option<i32> {
    std::fs::read_to_string(path).ok().and_then(|s| s.trim().parse().ok())
}

#[cfg(target_os = "linux")]
fn detect_numa_node(cpu_id: i32) -> i32 {
    // Try to find which NUMA node this CPU belongs to
    if let Ok(entries) = std::fs::read_dir("/sys/devices/system/node") {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(n) = name.strip_prefix("node") {
                if let Ok(node_id) = n.parse::<i32>() {
                    let cpulist_path = format!("/sys/devices/system/node/{}/cpulist", name);
                    if let Ok(cpulist) = std::fs::read_to_string(&cpulist_path) {
                        if cpu_in_list(cpu_id, &cpulist) {
                            return node_id;
                        }
                    }
                }
            }
        }
    }
    0 // Default to node 0
}

#[cfg(target_os = "linux")]
fn cpu_in_list(cpu_id: i32, list: &str) -> bool {
    for part in list.trim().split(',') {
        if part.contains('-') {
            let range: Vec<&str> = part.split('-').collect();
            if range.len() == 2 {
                if let (Ok(start), Ok(end)) = (range[0].parse::<i32>(), range[1].parse::<i32>()) {
                    if cpu_id >= start && cpu_id <= end {
                        return true;
                    }
                }
            }
        } else if let Ok(id) = part.parse::<i32>() {
            if id == cpu_id {
                return true;
            }
        }
    }
    false
}

#[cfg(target_os = "linux")]
fn detect_l3_cache_size(cpu_id: i32) -> u64 {
    // Look through cache indices for L3
    for index in 0..10 {
        let cache_path = format!("/sys/devices/system/cpu/cpu{}/cache/index{}", cpu_id, index);

        let level_path = format!("{}/level", cache_path);
        let size_path = format!("{}/size", cache_path);

        if let Ok(level) = std::fs::read_to_string(&level_path) {
            if level.trim() == "3" {
                if let Ok(size_str) = std::fs::read_to_string(&size_path) {
                    return parse_cache_size(&size_str);
                }
            }
        }
    }
    0
}

#[cfg(target_os = "linux")]
fn parse_cache_size(size_str: &str) -> u64 {
    let s = size_str.trim().to_uppercase();

    if let Some(kb) = s.strip_suffix('K') {
        kb.parse::<u64>().unwrap_or(0) * 1024
    } else if let Some(mb) = s.strip_suffix('M') {
        mb.parse::<u64>().unwrap_or(0) * 1024 * 1024
    } else if let Some(gb) = s.strip_suffix('G') {
        gb.parse::<u64>().unwrap_or(0) * 1024 * 1024 * 1024
    } else {
        s.parse::<u64>().unwrap_or(0)
    }
}

// Windows helper functions
#[cfg(target_os = "windows")]
fn get_windows_topology_info() -> (u32, u32, u32, u64) {
    /* UNTESTED:
    use std::mem;
    use windows::Win32::System::SystemInformation::{
        GetLogicalProcessorInformation, RelationCache, RelationNumaNode, RelationProcessorCore,
        RelationProcessorPackage, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
    };

    let mut packages = 1u32;
    let mut cores = 0u32;
    let mut nodes = 1u32;
    let mut l3_cache_size = 0u64;

    // Get required buffer size
    let mut buffer_size = 0u32;
    unsafe {
        let _ = GetLogicalProcessorInformation(None, &mut buffer_size);
    }

    if buffer_size == 0 {
        // Fallback to simple detection
        let logical = std::env::var("NUMBER_OF_PROCESSORS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        return (1, logical, 1, 0);
    }

    let count = buffer_size as usize / mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
    let mut buffer: Vec<SYSTEM_LOGICAL_PROCESSOR_INFORMATION> =
        vec![unsafe { mem::zeroed() }; count];

    let result =
        unsafe { GetLogicalProcessorInformation(Some(buffer.as_mut_ptr()), &mut buffer_size) };

    if result.is_ok() {
        let mut package_count = 0u32;
        let mut numa_count = 0u32;

        for info in &buffer {
            match info.Relationship {
                RelationProcessorCore => {
                    cores += 1;
                }
                RelationProcessorPackage => {
                    package_count += 1;
                }
                RelationNumaNode => {
                    numa_count += 1;
                }
                RelationCache => {
                    let cache = unsafe { info.Anonymous.Cache };
                    if cache.Level == 3 && l3_cache_size == 0 {
                        l3_cache_size = cache.Size as u64;
                    }
                }
                _ => {}
            }
        }

        if package_count > 0 {
            packages = package_count;
        }
        if numa_count > 0 {
            nodes = numa_count;
        }
    }

    // If cores is still 0, use logical count
    if cores == 0 {
        cores = std::env::var("NUMBER_OF_PROCESSORS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
    }

    (packages, cores, nodes, l3_cache_size)
    */
    (0, 0, 0, 0)
}
