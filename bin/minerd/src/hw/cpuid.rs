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

use std::fmt;

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
}

impl CpuInfo {
    /// Detect the CPU using CPUID instruction
    pub fn detect() -> Self {
        let (vendor_string, vendor) = get_vendor();
        let (family, model, stepping) = get_family_model_stepping();
        let brand_string = get_brand_string();

        let zen_generation =
            if vendor == Vendor::Amd { Some(detect_zen_generation(family, model)) } else { None };

        Self { vendor, vendor_string, family, model, stepping, brand_string, zen_generation }
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
