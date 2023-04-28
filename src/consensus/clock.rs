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

use crate::{util::time::Timestamp, Result};
use log::debug;
use std::{thread, time::Duration};
use url::Url;

pub enum Ticks {
    GENESIS { e: u64, sl: u64 },  //genesis epoch
    NEWSLOT { e: u64, sl: u64 },  // new slot
    NEWEPOCH { e: u64, sl: u64 }, // new epoch
    TOCKS,                        //tocks, or slot is ending
    IDLE,                         // idle clock state
    OUTOFSYNC,                    //clock, and blockchain are out of sync
}

const BB_SL: u64 = u64::MAX - 1; //big bang slot time (need to be negative value)
const BB_E: u64 = 0; //big bang epoch time.

#[derive(Debug)]
pub struct Clock {
    pub sl: u64,       // relative slot index (zero-based) [0-len[
    pub e: u64,        // epoch index (zero-based) [0-\inf[
    pub tick_len: u64, // tick length in time (seconds)
    pub sl_len: u64,   // slot length in ticks
    pub e_len: u64,    // epoch length in slots
    pub peers: Vec<Url>,
    pub genesis_time: Timestamp,
}

impl Clock {
    pub fn new(
        e_len: Option<u64>,
        sl_len: Option<u64>,
        tick_len: Option<u64>,
        peers: Vec<Url>,
    ) -> Self {
        let gt: Timestamp = Timestamp::current_time();
        Self {
            sl: BB_SL, //necessary for genesis slot
            e: BB_E,
            tick_len: tick_len.unwrap_or(22), // 22 seconds
            sl_len: sl_len.unwrap_or(22),     // ~8 minutes
            e_len: e_len.unwrap_or(3),        // 24.2 minutes
            peers,
            genesis_time: gt,
        }
    }

    pub fn get_sl_len(&self) -> u64 {
        self.sl_len
    }

    pub fn get_e_len(&self) -> u64 {
        self.e_len
    }

    async fn time(&self) -> Result<Timestamp> {
        //TODO (fix) add more than ntp server to time, and take the avg
        Ok(Timestamp::current_time())
    }

    /// returns time since genesis in seconds.
    async fn time_to_genesis(&self) -> Timestamp {
        //TODO this value need to be assigned to kickoff time.
        let genesis_time = self.genesis_time.0;
        let abs_time = self.time().await.unwrap();
        Timestamp(abs_time.0 - genesis_time)
    }

    /// return absolute tick to genesis, and relative tick index in the slot.
    async fn tick_time(&self) -> (u64, u64, u64) {
        let time = self.time_to_genesis().await.0 as u64;
        let tick_abs: u64 = time / self.tick_len;
        let tick_rel: u64 = time % self.tick_len;
        (time, tick_rel, tick_abs)
    }

    /// return true if the clock is at the begining (before 2/3 of the slot).
    async fn ticking(&self) -> bool {
        let (abs, rel, _) = self.tick_time().await;
        debug!(target: "consensus::clock", "abs time to genesis ticks: {}, rel ticks: {}", abs, rel);
        rel < (self.tick_len) * 2 / 3
    }

    pub async fn sync(&mut self) -> Result<()> {
        let e = self.epoch_abs().await;
        let sl = self.slot_relative().await;
        self.sl = sl;
        self.e = e;
        Ok(())
    }

    /// returns absolute zero based slot index
    async fn slot_abs(&self) -> u64 {
        let sl_abs = self.tick_time().await.0 / self.sl_len;
        debug!(target: "consensus::clock", "[slot_abs] slot len: {} - slot abs: {}", self.sl_len, sl_abs);
        sl_abs
    }

    /// returns relative zero based slot index
    async fn slot_relative(&self) -> u64 {
        let e_abs = self.slot_abs().await % self.e_len;
        debug!(target: "consensus::clock", "[slot_relative] slot len: {} - slot relative: {}", self.sl_len, e_abs);
        e_abs
    }

    /// returns absolute zero based epoch index.
    async fn epoch_abs(&self) -> u64 {
        let res = self.slot_abs().await / self.e_len;
        debug!(target: "consensus::clock", "[epoch_abs] epoch len: {} - epoch abs: {}", self.e_len, res);
        res
    }

    /// return the ticks phase with corresponding phase parameters
    ///
    /// the Ticks enum can include epoch index, and relative slot index (zero-based)
    pub async fn ticks(&mut self) -> Ticks {
        // also debug the failing function.
        let e = self.epoch_abs().await;
        let sl = self.slot_relative().await;
        if self.ticking().await {
            debug!(
                target: "consensus::clock",
                "e/e`: {}/{} sl/sl`: {}/{}, BB_E/BB_SL: {}/{}",
                e, self.e, sl, self.sl, BB_E, BB_SL
            );
            if e == self.e && e == BB_E && self.sl == BB_SL {
                self.sl = sl + 1; // 0
                self.e = e; // 0
                debug!(target: "consensus::clock", "new genesis");
                Ticks::GENESIS { e, sl }
            } else if e == self.e && sl == self.sl + 1 {
                self.sl = sl;
                debug!(target: "consensus::clock", "new slot");
                Ticks::NEWSLOT { e, sl }
            } else if e == self.e + 1 && sl == 0 {
                self.e = e;
                self.sl = sl;
                debug!(target: "consensus::clock", "new epoch");
                Ticks::NEWEPOCH { e, sl }
            } else if e == self.e && sl == self.sl {
                debug!(target: "consensus::clock", "clock is idle");
                thread::sleep(Duration::from_millis(100));
                Ticks::IDLE
            } else {
                debug!(target: "consensus::clock", "clock is out of sync");
                //clock is out of sync
                Ticks::OUTOFSYNC
            }
        } else {
            debug!(target: "consensus::clock", "tocks");
            Ticks::TOCKS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Clock, Ticks};
    use futures::executor::block_on;
    use std::{thread, time::Duration};
    #[test]
    fn clock_works() {
        let clock = Clock::new(Some(9), Some(9), Some(9), vec![]);
        //block th for 3 secs
        thread::sleep(Duration::from_millis(1000));
        let ttg = block_on(clock.time_to_genesis()).0;
        assert!((1..2).contains(&ttg));
    }

    fn _clock_ticking() {
        let clock = Clock::new(Some(9), Some(9), Some(9), vec![]);
        //block th for 3 secs
        thread::sleep(Duration::from_millis(1000));
        assert!(block_on(clock.ticking()));
        thread::sleep(Duration::from_millis(1000));
        assert!(block_on(clock.ticking()));
    }

    fn _clock_ticks() {
        let mut clock = Clock::new(Some(9), Some(9), Some(9), vec![]);
        //
        let tick: Ticks = block_on(clock.ticks());
        assert!(matches!(tick, Ticks::GENESIS { e: 0, sl: 0 }));
        thread::sleep(Duration::from_millis(3000));
        let tock: Ticks = block_on(clock.ticks());
        assert!(matches!(tock, Ticks::TOCKS));
    }
}
