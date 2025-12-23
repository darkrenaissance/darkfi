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

use std::{fmt, str::FromStr};

/// Sentinel value indicating no mask should be applied
pub const NO_MASK: u64 = u64::MAX;

/// Represents a single MSR operation.
///
/// Contains the register address, value to write/read, and an optional
/// mask for partial register updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsrItem {
    /// The MSR register address
    reg: u32,
    /// The value to read/write
    value: u64,
    /// Mask for partial updates (`NO_MASK` means full update)
    mask: u64,
}

impl Default for MsrItem {
    fn default() -> Self {
        Self { reg: 0, value: 0, mask: NO_MASK }
    }
}

impl MsrItem {
    /// Create a new MsrItem with optional mask
    pub const fn new(reg: u32, value: u64) -> Self {
        Self { reg, value, mask: NO_MASK }
    }

    /// Create a new MsrItem with a specific mask
    pub const fn with_mask(reg: u32, value: u64, mask: u64) -> Self {
        Self { reg, value, mask }
    }

    /// Check if this item is valid (register > 0)
    #[inline]
    pub const fn is_valid(&self) -> bool {
        self.reg > 0
    }

    /// Get the register address
    #[inline]
    pub const fn reg(&self) -> u32 {
        self.reg
    }

    /// Get the value
    #[inline]
    pub const fn value(&self) -> u64 {
        self.value
    }

    /// Get the mask
    #[inline]
    pub const fn mask(&self) -> u64 {
        self.mask
    }

    /// Check if this item has a mask
    #[inline]
    pub const fn has_mask(&self) -> bool {
        self.mask != NO_MASK
    }

    /// Apply mask to combine old and new values.
    ///
    /// The masked bits from `new_value` replace the corresponding bits in
    /// `old_value`, while unmasked bits retain the `old_value`.
    ///
    /// ```text
    /// let old = 0xFF00_FF00;
    /// let new = 0x1234_5678;
    /// let mask = 0xFFFF_0000;
    ///
    /// // Upper 16 bits from new, lower 16 bits from old:
    /// assert_eq!(MsrItem::masked_value(old, new, mask), 0x1234_FF00);
    /// ```
    #[inline]
    pub const fn masked_value(old_value: u64, new_value: u64, mask: u64) -> u64 {
        (new_value & mask) | (old_value & !mask)
    }

    /// Set the value, useful for updating after a read
    #[inline]
    pub fn set_value(&mut self, value: u64) {
        self.value = value;
    }
}

/// Error type for parsing MsrItem from a string
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseMsrItemError {
    pub message: String,
}

impl fmt::Display for ParseMsrItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Failed to parse MsrItem: {}", self.message)
    }
}

impl std::error::Error for ParseMsrItemError {}

impl FromStr for MsrItem {
    type Err = ParseMsrItemError;

    /// Parse an MsrItem from a string in format `REG:VALUE` or `REG:VALUE:MASK`
    ///
    /// Values can be decimal or hexadecimal.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();

        if parts.len() < 2 {
            return Err(ParseMsrItemError {
                message: "Expected format REG:VALUE or REG:VALUE:MASK".to_string(),
            })
        }

        let reg = parse_number(parts[0])
            .map_err(|e| ParseMsrItemError { message: format!("Invalid register: {}", e) })?
            as u32;

        let value = parse_number(parts[1])
            .map_err(|e| ParseMsrItemError { message: format!("Invalid value: {}", e) })?;

        let mask = if parts.len() > 2 {
            parse_number(parts[2])
                .map_err(|e| ParseMsrItemError { message: format!("Invalid mask: {}", e) })?
        } else {
            NO_MASK
        };

        Ok(Self { reg, value, mask })
    }
}

/// Parse a number from string, supporting decimal and hexadecimal
fn parse_number(s: &str) -> Result<u64, std::num::ParseIntError> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
    } else {
        s.parse::<u64>()
    }
}

impl fmt::Display for MsrItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mask != NO_MASK {
            write!(f, "0x{:x}:0x{:x}:0x{:x}", self.reg, self.value, self.mask)
        } else {
            write!(f, "0x{:x}:0x{:x}", self.reg, self.value)
        }
    }
}
