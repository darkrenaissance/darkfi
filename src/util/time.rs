/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

const SECS_IN_DAY: u64 = 86400;
const MIN_IN_HOUR: u64 = 60;
const SECS_IN_HOUR: u64 = 3600;

/// Wrapper struct to represent system timestamps.
#[derive(Hash, Clone, Copy, Debug, SerialEncodable, SerialDecodable, PartialEq, PartialOrd, Eq)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Generate a `Timestamp` of the current time.
    pub fn current_time() -> Self {
        Self(UNIX_EPOCH.elapsed().unwrap().as_secs())
    }

    /// Calculates elapsed time of a `Timestamp`.
    /// TODO: Rework this function to return the result of checked_sub and make calling code
    /// check whether it is Some/None
    pub fn elapsed(&self) -> u64 {
        let now = UNIX_EPOCH.elapsed().unwrap().as_secs();
        if let Some(elapsed) = now.checked_sub(self.0) {
            elapsed
        } else {
            panic!(
                "Cannot subtract Timestamp value {} from current time {}. (Integer underflow)",
                self.0, now
            );
        }
    }

    /// Increment a 'Timestamp'.
    /// TODO: Rework this function to return the result of checked_add and make calling code
    /// check whether it is Some/None
    pub fn add(&mut self, inc: u64) {
        if let Some(sum) = self.0.checked_add(inc) {
            self.0 = sum
        } else {
            panic!("Cannot add {} to Timestamp {}. (Integer overflow)", self.0, inc);
        }
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
    pub fn current_time() -> Self {
        Self(UNIX_EPOCH.elapsed().unwrap().as_nanos())
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

#[derive(Clone, Debug, Default)]
pub struct DateTime {
    pub nanos: u32,
    pub sec: u32,
    pub min: u32,
    pub hour: u32,
    pub day: u32,
    pub month: u32,
    pub year: u32,
}

impl DateTime {
    pub fn new() -> Self {
        Self { nanos: 0, sec: 0, min: 0, hour: 0, day: 0, month: 0, year: 0 }
    }

    pub fn date(&self) -> Date {
        Date { day: self.day, month: self.month, year: self.year }
    }

    pub fn from_timestamp(secs: u64, nsecs: u32) -> Self {
        let leap_year = |year| -> bool { year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) };

        static MONTHS: [[u64; 12]; 2] = [
            [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
            [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
        ];

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
        while day_number >= MONTHS[if leap_year(year) { 1 } else { 0 }][month] {
            day_number -= MONTHS[if leap_year(year) { 1 } else { 0 }][month];
            month += 1;
        }
        date_time.month = month as u32 + 1;
        date_time.day = day_number as u32 + 1;

        date_time
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
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
        write!(f, "{:04}-{:02}-{:02} UTC", self.year, self.month, self.day)
    }
}

pub fn timestamp_to_date(timestamp: u64, format: DateFormat) -> String {
    if timestamp == 0 {
        return "".to_string()
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
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{} UTC",
                dt.year, dt.month, dt.day, dt.hour, dt.min, dt.sec, dt.nanos
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Timestamp;

    #[test]
    #[should_panic]
    fn panic_on_add_overflow() {
        // Panic when the Timestamp func add() overflows u64.
        let mut ts = Timestamp::current_time();
        ts.add(u64::MAX);
    }

    #[test]
    #[should_panic]
    fn panic_on_elapsed_underflow() {
        // Panic when the Timestamp function elapsed() underflows u64.
        let mut ts = Timestamp::current_time();
        ts.add(10_000);
        ts.elapsed();
    }
}
