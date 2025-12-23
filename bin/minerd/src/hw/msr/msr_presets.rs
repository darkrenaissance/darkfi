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

use super::{super::cpuid::AmdZenGeneration, MsrItem};

/// MSR preset index for supported CPU types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum MsrPreset {
    /// No preset (empty)
    None = 0,
    /// AMD Zen1 / Zen+ / Zen2
    Zen1Zen2 = 1,
    /// AMD Zen 3
    Zen3 = 2,
    /// AMD Zen 4
    Zen4 = 3,
    /// AMD Zen 5
    Zen5 = 4,
    /// Intel
    Intel = 5,
    /// Maximum value
    Max = 6,
}

impl MsrPreset {
    /// Get preset from AMD Zen generation
    pub fn from_zen(gen: AmdZenGeneration) -> Self {
        match gen {
            AmdZenGeneration::Zen1OrZen2 => MsrPreset::Zen1Zen2,
            AmdZenGeneration::Zen3 => MsrPreset::Zen3,
            AmdZenGeneration::Zen4 => MsrPreset::Zen4,
            AmdZenGeneration::Zen5 => MsrPreset::Zen5,
            AmdZenGeneration::Unknown => MsrPreset::None,
        }
    }
}

/// Total number of MSR presets
pub const MSR_ARRAY_SIZE: usize = MsrPreset::Max as usize + 1;

/// Mask that clears bit 5 (used for 0xC0011021 register)
/// This is `~0x20ULL` in C++
const MASK_CLEAR_BIT5: u64 = !0x20u64; // 0xFFFFFFFFFFFFFFDF

/// Static array of MSR presets for each CPU type
///
/// Index corresponds to `MsrPreset` enum values.
pub static MSR_PRESETS: [&[MsrItem]; MSR_ARRAY_SIZE] = [
    // 0: None - empty preset
    &[],
    // 1: Zen1/Zen2
    &[
        MsrItem::new(0xc0011020, 0x0),
        MsrItem::with_mask(0xc0011021, 0x40, MASK_CLEAR_BIT5),
        MsrItem::new(0xc0011022, 0x1510000),
        MsrItem::new(0xc001102b, 0x2000cc16),
    ],
    // 2: Zen3
    &[
        MsrItem::new(0xc0011020, 0x0004480000000000),
        MsrItem::with_mask(0xc0011021, 0x001c000200000040, MASK_CLEAR_BIT5),
        MsrItem::new(0xc0011022, 0xc000000401570000),
        MsrItem::new(0xc001102b, 0x2000cc10),
    ],
    // 3: Zen4
    &[
        MsrItem::new(0xc0011020, 0x0004400000000000),
        MsrItem::with_mask(0xc0011021, 0x0004000000000040, MASK_CLEAR_BIT5),
        MsrItem::new(0xc0011022, 0x8680000401570000),
        MsrItem::new(0xc001102b, 0x2040cc10),
    ],
    // 4: Zen5
    &[
        MsrItem::new(0xc0011020, 0x0004400000000000),
        MsrItem::with_mask(0xc0011021, 0x0004000000000040, MASK_CLEAR_BIT5),
        MsrItem::new(0xc0011022, 0x8680000401570000),
        MsrItem::new(0xc001102b, 0x2040cc10),
    ],
    // 5: Intel
    &[MsrItem::new(0x1a4, 0xf)],
    // 6: Max - empty sentinel
    &[],
];

/// Get MSR items for a specific preset
#[inline]
pub fn get_preset(preset: MsrPreset) -> &'static [MsrItem] {
    MSR_PRESETS[preset as usize]
}
