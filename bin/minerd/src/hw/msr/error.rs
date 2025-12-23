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

use std::{fmt, io};

pub type MsrResult<T> = Result<T, MsrError>;

/// Errors that can occur during MSR operations
#[derive(Debug)]
pub enum MsrError {
    /// MSR module/driver is not available
    NotAvailable(String),

    /// Failed to read MSR
    ReadError { reg: u32, cpu: i32, source: io::Error },

    /// Failed to write MSR
    WriteError { reg: u32, cpu: i32, source: io::Error },

    /// No CPU units available
    NoCpuUnits,

    /// Permission denied
    PermissionDenied(String),

    /// Driver installation failed (Windows)
    DriverError(String),

    /// Generic IO error
    Io(io::Error),

    /// Platform not supported
    PlatformNotSupported,
}

impl fmt::Display for MsrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MsrError::NotAvailable(msg) => write!(f, "MSR not available: {}", msg),
            MsrError::ReadError { reg, cpu, source } => {
                write!(f, "Failed to read MSR 0x{:08x} on CPU {}: {}", reg, cpu, source)
            }
            MsrError::WriteError { reg, cpu, source } => {
                write!(f, "Failed to write MSR 0x{:08x} on CPU {}: {}", reg, cpu, source)
            }
            MsrError::NoCpuUnits => write!(f, "No CPU units available"),
            MsrError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            MsrError::DriverError(msg) => write!(f, "Driver error: {}", msg),
            MsrError::Io(err) => write!(f, "IO error: {}", err),
            MsrError::PlatformNotSupported => write!(f, "Platform not supported"),
        }
    }
}

impl std::error::Error for MsrError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MsrError::ReadError { source, .. } => Some(source),
            MsrError::WriteError { source, .. } => Some(source),
            MsrError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for MsrError {
    fn from(err: io::Error) -> Self {
        MsrError::Io(err)
    }
}
