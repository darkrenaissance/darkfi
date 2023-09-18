/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use crate::Result;

const SECS_IN_DAY: u64 = 86400;
const MIN_IN_HOUR: u64 = 60;
const SECS_IN_HOUR: u64 = 3600;

/// Helper structure providing time related calculations.
/// This struct is optimized for performance and does not check
/// its arithmetic: division-by-zero is possible for certain values.
/// [`TimeKeeperSafe`] should be used if safety is more important than
/// performance.
#[derive(Clone)]
pub struct TimeKeeper {
    /// Genesis block creation timestamp
    pub genesis_ts: Timestamp,
    /// Currently configured epoch duration
    pub epoch_length: u64,
    /// Currently configured slot duration
    pub slot_time: u64,
    /// Slot number runtime can access to verify against
    pub verifying_slot: u64,
}

impl TimeKeeper {
    pub fn new(
        genesis_ts: Timestamp,
        epoch_length: u64,
        slot_time: u64,
        verifying_slot: u64,
    ) -> Self {
        Self { genesis_ts, epoch_length, slot_time, verifying_slot }
    }

    /// Generate a TimeKeeper for current slot
    pub fn current(&self) -> Self {
        Self {
            genesis_ts: self.genesis_ts,
            epoch_length: self.epoch_length,
            slot_time: self.slot_time,
            verifying_slot: self.current_slot(),
        }
    }

    /// Calculates current epoch.
    pub fn current_epoch(&self) -> u64 {
        self.slot_epoch(self.current_slot())
    }

    /// Calculates the epoch of the provided slot.
    /// Only slot 0 exists in epoch 0, everything
    /// else is incremented by one. This practically
    /// means that epoch 0 has 1 slot(the genesis slot),
    /// epoch 1 has one less slot(the genesis slot) and
    /// rest epoch have the normal amount of slots.
    pub fn slot_epoch(&self, slot: u64) -> u64 {
        if slot == 0 {
            return 0
        }
        (slot / self.epoch_length) + 1
    }

    /// Calculates current slot, based on elapsed time from the genesis block.
    pub fn current_slot(&self) -> u64 {
        self.genesis_ts.elapsed() / self.slot_time
    }

    /// Calculates the relative number of the provided slot.
    pub fn relative_slot(&self, slot: u64) -> u64 {
        slot % self.epoch_length
    }

    /// Calculates the epoch of the verifying slot.
    pub fn verifying_slot_epoch(&self) -> u64 {
        self.slot_epoch(self.verifying_slot)
    }

    /// Calculates seconds until next Nth slot starting time.
    pub fn next_n_slot_start(&self, n: u64) -> u64 {
        assert!(n > 0);
        let next_slot_start = self.genesis_ts.0 + (self.current_slot() + n) * self.slot_time;
        next_slot_start - Timestamp::current_time().0
    }

    /// Calculate slots until next Nth epoch.
    /// Epoch duration is configured using the EPOCH_LENGTH value.
    pub fn slots_to_next_n_epoch(&self, n: u64) -> u64 {
        assert!(n > 0);
        let slots_till_next_epoch = self.epoch_length - self.relative_slot(self.current_slot());
        ((n - 1) * self.epoch_length) + slots_till_next_epoch
    }

    /// Calculates seconds until next Nth epoch starting time.
    pub fn next_n_epoch_start(&self, n: u64) -> u64 {
        self.next_n_slot_start(self.slots_to_next_n_epoch(n))
    }

    /// Calculates current blockchain timestamp.
    /// Blockchain timestamp is the time elapsed since
    /// Genesis timestamp, based on slot time ticking,
    /// therefore representing the starting timestamp of
    /// current slot.
    pub fn blockchain_timestamp(&self) -> u64 {
        self.genesis_ts.0 + self.current_slot() * self.slot_time
    }

    /// Calculates current system timestamp.
    pub fn system_timestamp(&self) -> Result<u64> {
        Ok(UNIX_EPOCH.elapsed()?.as_secs())
    }
}
/// Wrapper struct that allows only coherent and safe values for a [`TimeKeeper`].
#[derive(Clone)]
pub struct TimeKeeperSafe {
    timekeeper: TimeKeeper,
}

impl TimeKeeperSafe {
    pub fn new(
        genesis_ts: Timestamp,
        epoch_length: u64,
        slot_time: u64,
        verifying_slot: u64,
    ) -> Self {
        // TimeKeeper uses epoch_length and slot_time as divisors so they should
        // never be zero in this struct.
        if epoch_length == 0 {
            panic!("Epoch length cannot be zero");
        }
        if slot_time == 0 {
            panic!("Slot time cannot be zero");
        }
        Self { timekeeper: TimeKeeper { genesis_ts, epoch_length, slot_time, verifying_slot } }
    }
    /// Generate a TimeKeeperSafe for current slot
    pub fn current(&self) -> TimeKeeperSafe {
        TimeKeeperSafe::new(
            self.timekeeper.genesis_ts,
            self.timekeeper.epoch_length,
            self.timekeeper.slot_time,
            self.timekeeper.verifying_slot,
        )
    }

    /// Calculates current epoch.
    pub fn current_epoch(&self) -> u64 {
        self.timekeeper.current_epoch()
    }

    /// Calculates the epoch of the provided slot.
    /// Only slot 0 exists in epoch 0, everything
    /// else is incremented by one. This practically
    /// means that epoch 0 has 1 slot(the genesis slot),
    /// epoch 1 has one less slot(the genesis slot) and
    /// rest epoch have the normal amount of slots.
    pub fn slot_epoch(&self, slot: u64) -> u64 {
        if self.timekeeper.epoch_length == 0 {
            panic!("Epoch length cannot be zero");
        }
        self.timekeeper.slot_epoch(slot)
    }

    /// Calculates current slot, based on elapsed time from the genesis block.
    pub fn current_slot(&self) -> u64 {
        if self.timekeeper.slot_time == 0 {
            panic!("Slot time cannot be zero");
        }
        self.timekeeper.current_slot()
    }

    /// Calculates the relative number of the provided slot.
    pub fn relative_slot(&self, slot: u64) -> u64 {
        if self.timekeeper.epoch_length == 0 {
            panic!("Epoch length cannot be zero");
        }
        self.timekeeper.relative_slot(slot)
    }

    /// Calculates the epoch of the verifying slot.
    pub fn verifying_slot_epoch(&self) -> u64 {
        self.timekeeper.verifying_slot_epoch()
    }

    /// Calculates seconds until next Nth slot starting time.
    pub fn next_n_slot_start(&self, n: u64) -> u64 {
        self.timekeeper.next_n_slot_start(n)
    }

    /// Calculate slots until next Nth epoch.
    /// Epoch duration is configured using the EPOCH_LENGTH value.
    pub fn slots_to_next_n_epoch(&self, n: u64) -> u64 {
        self.timekeeper.slots_to_next_n_epoch(n)
    }

    /// Calculates seconds until next Nth epoch starting time.
    pub fn next_n_epoch_start(&self, n: u64) -> u64 {
        self.timekeeper.next_n_epoch_start(n)
    }

    /// Calculates current blockchain timestamp.
    /// Blockchain timestamp is the time elapsed since
    /// Genesis timestamp, based on slot time ticking,
    /// therefore representing the starting timestamp of
    /// current slot.
    pub fn blockchain_timestamp(&self) -> u64 {
        self.timekeeper.blockchain_timestamp()
    }

    /// Calculates current system timestamp.
    pub fn system_timestamp(&self) -> Result<u64> {
        self.timekeeper.system_timestamp()
    }
}

/// Wrapper struct to represent system timestamps.
#[derive(Hash, Clone, Copy, Debug, SerialEncodable, SerialDecodable, PartialEq, PartialOrd, Eq)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Generate a `Timestamp` of the current time.
    pub fn current_time() -> Self {
        Self(UNIX_EPOCH.elapsed().unwrap().as_secs())
    }

    /// Calculates elapsed time of a `Timestamp`.
    pub fn elapsed(&self) -> u64 {
        UNIX_EPOCH.elapsed().unwrap().as_secs() - self.0
    }

    /// Increment a 'Timestamp'.
    pub fn add(&mut self, inc: u64) {
        self.0 += inc;
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
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
impl std::fmt::Display for NanoTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
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
    use super::{TimeKeeper, TimeKeeperSafe, Timestamp};

    #[test]
    #[should_panic]
    fn panic_on_unsafe_epoch_length() {
        // Ensure panic when epoch_length is 0.
        TimeKeeperSafe::new(Timestamp::current_time(), 0, 1, 1);
    }

    #[test]
    #[should_panic]
    fn panic_on_unsafe_slot_time() {
        TimeKeeperSafe::new(Timestamp::current_time(), 1, 0, 1);
    }

    #[test]
    #[should_panic]
    fn panic_on_unsafe_call_to_method_current_slot() {
        // Test against manual initialization of unsafe TimeKeeper fields.
        let tk_unsafe =
            TimeKeeperSafe { timekeeper: TimeKeeper::new(Timestamp::current_time(), 0, 0, 0) };
        tk_unsafe.current_slot();
    }

    #[test]
    #[should_panic]
    fn panic_on_unsafe_call_to_method_relative_slot() {
        // Test against manual initialization of unsafe TimeKeeper fields.
        let tk_unsafe =
            TimeKeeperSafe { timekeeper: TimeKeeper::new(Timestamp::current_time(), 0, 0, 0) };
        tk_unsafe.relative_slot(0);
    }

    #[test]
    #[should_panic]
    fn panic_on_unsafe_call_to_method_slot_epoch() {
        // Test against manual initialization of unsafe TimeKeeper fields.
        let tk_unsafe =
            TimeKeeperSafe { timekeeper: TimeKeeper::new(Timestamp::current_time(), 0, 0, 0) };
        tk_unsafe.slot_epoch(0);
    }
}
