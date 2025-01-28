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

use std::{fmt, time::UNIX_EPOCH};

#[cfg(feature = "async-serial")]
use darkfi_serial::async_trait;

use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::{Error, Result};

const SECS_IN_DAY: u64 = 86400;
const MIN_IN_HOUR: u64 = 60;
const SECS_IN_HOUR: u64 = 3600;
/// Represents the number of days in each month for both leap and non-leap years.
const DAYS_IN_MONTHS: [[u64; 12]; 2] = [
    [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31], // Leap years
];

/// Wrapper struct to represent system timestamps.
#[derive(
    Hash,
    Clone,
    Copy,
    Debug,
    SerialEncodable,
    SerialDecodable,
    PartialEq,
    PartialOrd,
    Ord,
    Eq,
    Default,
)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Returns the inner `u64` of `Timestamp`
    pub fn inner(&self) -> u64 {
        self.0
    }

    /// Generate a `Timestamp` of the current time.
    pub fn current_time() -> Self {
        Self(UNIX_EPOCH.elapsed().unwrap().as_secs())
    }

    /// Calculates the elapsed time of a `Timestamp` up to the time of calling the function.
    pub fn elapsed(&self) -> Result<Self> {
        Self::current_time().checked_sub(*self)
    }

    /// Add `self` to a given timestamp
    /// Errors on integer overflow.
    pub fn checked_add(&self, ts: Self) -> Result<Self> {
        if let Some(result) = self.inner().checked_add(ts.inner()) {
            Ok(Self(result))
        } else {
            Err(Error::AdditionOverflow)
        }
    }

    /// Subtract `self` with a given timestamp
    /// Errors on integer underflow.
    pub fn checked_sub(&self, ts: Self) -> Result<Self> {
        if let Some(result) = self.inner().checked_sub(ts.inner()) {
            Ok(Self(result))
        } else {
            Err(Error::SubtractionUnderflow)
        }
    }

    pub const fn from_u64(x: u64) -> Self {
        Self(x)
    }
}

impl From<u64> for Timestamp {
    fn from(x: u64) -> Self {
        Self(x)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let date = timestamp_to_date(self.0, DateFormat::DateTime);
        write!(f, "{}", date)
    }
}

#[derive(Clone, Copy, Debug, SerialEncodable, SerialDecodable, PartialEq, PartialOrd, Eq)]
pub struct NanoTimestamp(pub u128);

impl NanoTimestamp {
    pub fn inner(&self) -> u128 {
        self.0
    }

    pub fn current_time() -> Self {
        Self(UNIX_EPOCH.elapsed().unwrap().as_nanos())
    }

    pub fn elapsed(&self) -> Result<Self> {
        Self::current_time().checked_sub(*self)
    }

    pub fn checked_sub(&self, ts: Self) -> Result<Self> {
        if let Some(result) = self.inner().checked_sub(ts.inner()) {
            Ok(Self(result))
        } else {
            Err(Error::SubtractionUnderflow)
        }
    }
}
impl fmt::Display for NanoTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let date = timestamp_to_date(self.0.try_into().unwrap(), DateFormat::Nanos);
        write!(f, "{}", date)
    }
}

pub enum DateFormat {
    Default,
    Date,
    DateTime,
    Nanos,
}

/// Represents a UTC `DateTime` with individual fields for date and time components.
#[derive(Clone, Debug, Default, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct DateTime {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
    pub nanos: u32,
}

impl DateTime {
    pub fn new() -> Self {
        Self { year: 0, month: 0, day: 0, hour: 0, min: 0, sec: 0, nanos: 0 }
    }

    pub fn date(&self) -> Date {
        Date { year: self.year, month: self.month, day: self.day }
    }

    pub fn from_timestamp(secs: u64, nsecs: u32) -> Self {
        let leap_year = |year| -> bool { year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) };

        let mut date_time = DateTime::new();
        let mut year = 1970;

        let time = secs % SECS_IN_DAY;
        let mut day_number = secs / SECS_IN_DAY;

        date_time.nanos = nsecs;
        date_time.sec = (time % MIN_IN_HOUR) as u32;
        date_time.min = ((time % SECS_IN_HOUR) / MIN_IN_HOUR) as u32;
        date_time.hour = (time / SECS_IN_HOUR) as u32;

        loop {
            let year_size = if leap_year(year) { 366 } else { 365 };
            if day_number >= year_size {
                day_number -= year_size;
                year += 1;
            } else {
                break
            }
        }
        date_time.year = year;

        let mut month = 0;
        while day_number >= DAYS_IN_MONTHS[if leap_year(year) { 1 } else { 0 }][month] {
            day_number -= DAYS_IN_MONTHS[if leap_year(year) { 1 } else { 0 }][month];
            month += 1;
        }
        date_time.month = month as u32 + 1;
        date_time.day = day_number as u32 + 1;

        date_time
    }

    /// Provides a `DateTime` instance from a string in "YYYY-MM-DDTHH:mm:ss" format.
    ///
    /// This function parses and validates the timestamp string, returning a `DateTime` instance
    /// with the parsed year, month, day, hour, minute, and second. Nanoseconds are not included
    /// in the input string and default to zero. If the input string does not match the expected
    /// format or contains invalid date or time values, it returns an [`Error::ParseFailed`] error.
    pub fn from_timestamp_str(timestamp_str: &str) -> Result<Self> {
        // Split the input string into date and time based on the 'T' separator
        let parts: Vec<&str> = timestamp_str.split('T').collect();

        // Check if the split parts have the correct length
        if parts.len() != 2 {
            return Err(Error::ParseFailed("Invalid timestamp format"));
        }

        // Parse the date into a vec
        let date_components: Vec<u32> = parts[0]
            .split('-')
            .map(|s| s.parse::<u32>().map_err(|_| Error::ParseFailed("Invalid date component")))
            .collect::<Result<Vec<u32>>>()?;

        // Verify year, month, and day are provided
        if date_components.len() != 3 {
            return Err(Error::ParseFailed("Invalid date format"));
        }

        // Parse the time into a vec
        let time_components: Vec<u32> = parts[1]
            .split(':')
            .map(|s| s.parse::<u32>().map_err(|_| Error::ParseFailed("Invalid time component")))
            .collect::<Result<Vec<u32>>>()?;

        // Verify that hour, minute, second are provided
        if time_components.len() != 3 {
            return Err(Error::ParseFailed("Invalid time format"));
        }

        // Destructure the date components into year, month, and day
        let (year, month, day) = (date_components[0], date_components[1], date_components[2]);

        // Validate month and day
        if !(1..=12).contains(&month) || !Self::is_valid_day(year, month, day) {
            return Err(Error::ParseFailed("Invalid month or day"));
        }

        // Destructure the time components into hour, minute, and second
        let (hour, min, sec) = (time_components[0], time_components[1], time_components[2]);

        // Validate hour, minute, and second values
        if hour > 23 || min > 59 || sec > 59 {
            return Err(Error::ParseFailed("Invalid hour, minute or second"));
        }

        // Return a new DateTime instance with parsed values and default nanoseconds set to 0
        Ok(DateTime { year, month, day, hour, min, sec, nanos: 0 })
    }

    /// Auxiliary function that determines whether the specified day is within the valid range
    /// for the given month and year, accounting for leap years. It returns `true` if the day
    /// is valid.
    fn is_valid_day(year: u32, month: u32, day: u32) -> bool {
        let days_in_month = DAYS_IN_MONTHS
            [(year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)) as usize]
            [(month - 1) as usize];
        day > 0 && day <= days_in_month as u32
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.min, self.sec
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct Date {
    pub day: u32,
    pub month: u32,
    pub year: u32,
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

// TODO: fix logic and add corresponding test case
pub fn timestamp_to_date(timestamp: u64, format: DateFormat) -> String {
    if timestamp == 0 {
        return "".to_string();
    }

    match format {
        DateFormat::Default => "".to_string(),
        DateFormat::Date => DateTime::from_timestamp(timestamp, 0).date().to_string(),
        DateFormat::DateTime => DateTime::from_timestamp(timestamp, 0).to_string(),
        DateFormat::Nanos => {
            const A_BILLION: u64 = 1_000_000_000;
            let dt =
                DateTime::from_timestamp(timestamp / A_BILLION, (timestamp % A_BILLION) as u32);
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{}",
                dt.year, dt.month, dt.day, dt.hour, dt.min, dt.sec, dt.nanos
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DateTime, Timestamp};

    #[test]
    fn check_ts_add_overflow() {
        assert!(Timestamp::current_time().checked_add(u64::MAX.into()).is_err());
    }

    #[test]
    fn check_ts_sub_underflow() {
        let cur = Timestamp::current_time().checked_add(10_000.into()).unwrap();
        assert!(cur.elapsed().is_err());
    }

    #[test]
    /// Tests the `from_timestamp_str` function to ensure it correctly converts timestamp strings into `DateTime` instances.
    fn test_from_timestamp_str() {
        // Verify validate dates
        let valid_timestamps = vec![
            (
                "2024-01-01T12:00:00",
                DateTime { year: 2024, month: 1, day: 1, hour: 12, min: 0, sec: 0, nanos: 0 },
            ),
            (
                "2024-02-29T23:59:59",
                DateTime { year: 2024, month: 2, day: 29, hour: 23, min: 59, sec: 59, nanos: 0 },
            ), // Leap year
            (
                "2023-12-31T00:00:00",
                DateTime { year: 2023, month: 12, day: 31, hour: 0, min: 0, sec: 0, nanos: 0 },
            ),
            (
                "1970-01-01T00:00:00",
                DateTime { year: 1970, month: 1, day: 1, hour: 0, min: 0, sec: 0, nanos: 0 },
            ), // Unix epoch
        ];

        for (timestamp_str, expected) in valid_timestamps {
            let result = DateTime::from_timestamp_str(timestamp_str)
                .expect("Valid timestamp should not fail");
            assert_eq!(result, expected);
        }

        // Verify boundary conditions
        let boundary_timestamps = vec![
            (
                "2023-02-28T23:59:59",
                DateTime { year: 2023, month: 2, day: 28, hour: 23, min: 59, sec: 59, nanos: 0 },
            ),
            (
                "2023-03-01T00:00:00",
                DateTime { year: 2023, month: 3, day: 1, hour: 0, min: 0, sec: 0, nanos: 0 },
            ),
            (
                "2024-02-29T12:30:30",
                DateTime { year: 2024, month: 2, day: 29, hour: 12, min: 30, sec: 30, nanos: 0 },
            ), // Leap year
        ];

        for (timestamp_str, expected) in boundary_timestamps {
            let result = DateTime::from_timestamp_str(timestamp_str)
                .expect("Valid timestamp should not fail");
            assert_eq!(result, expected);
        }

        // Verify invalid timestamps
        let invalid_timestamps = vec![
            "2023-02-30T12:00:00",    // Invalid day
            "2023-04-31T12:00:00",    // Invalid day
            "2023-13-01T12:00:00",    // Invalid month
            "2023-01-01T12.00.00",    // Invalid format
            "2023-01-01",             // Missing time part
            "2023-01-01 12.00.00",    // Missing T separator
            "2023/01/01T12:00",       // Incorrect date separator
            "2023-01-01T-12:-60:-60", // Invalid time components
        ];

        for timestamp_str in invalid_timestamps {
            let result = DateTime::from_timestamp_str(timestamp_str);
            assert!(result.is_err(), "Expected error for invalid timestamp '{}'", timestamp_str);
        }
    }
}
