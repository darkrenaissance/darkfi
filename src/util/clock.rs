use url::Url;

use crate::{
    util::time,
};

enum Ticks {
    GENESIS, //genesis epoch
    NEWSLOT{e: u8, sl: u8}, // new slot
    NEWEPOCH{e: u8, sl: u8}, // new epoch
    TOCKS, //tocks, or slot is ending
    IDLE, // idle clock state
    OUTOFSYNC, //clock, and blockchain are out of sync
}
const BB_SL : i64 = -1; //big bang slot time (need to be negative value)
const BB_E : i64 = -1; //big bang epoch time.

#[derive(Debug)]
pub struct Clock {
    pub sl : i64, // relative slot index (zero-based) [0-len[
    pub e : i64, //epoch index (zero-based) [0-\inf[
    pub tick_len: u8, // tick length in time
    pub sl_len: u8, // slot length in ticks
    pub e_len: u8, // epoch length in slots
    pub peers: Vec<Url>,

}

impl Clock {
    pub fn new(e_len: Option<u8>, sl_len: Option<u8>, tick_len: Option<u8>, peers: Vec<Url>) {
        Self { sl: BB_SL, //necessary for genesis slot
               e: BB_E,
               tick_len: tick_len.unwrap_or(22), // 22 seconds
               sl_len: sl_len.unwrap_or(22),// ~8 minutes
               e_len: e_len.unwrap_or(3), // 24.2 minutes
               len: len,
               peers: peers,
        }
    }

    fn time(&self) -> Result<Timestamp>{
        match time::check_clock(self.peers).await {
            Ok =>  {
                Some(time::ntp_request())
            }
        }
    }

    /// time since genesis
    fn time_to_genesis(&self) -> Timestamp {
        let genesis_time : u64 = 0;
        let abs_time = self.time().unwrap() {
            Err(e) => {
                !debug("time retrival fails, error: {}", e);
            }
        }.unwrap();
        abs_time - genesis_time
    }

    fn tick_time(&self) -> (i64, i64) {
        let time = self.time_to_genesis();
        let time_i = time.0;
        let tick_abs = (time_i / self.tick_len).abs();
        let tick_rel = time_i % self.tick_len;
        (tick_abs, tick_rel)
    }
    /// absolute slot ticks
    fn tick_abs(&self) -> i64 {
        self.tick_time().0
    }

    /// return true if the clock is at the begining of the slot
    fn ticking() -> bool{
        let (abs, rel) =  self.tick_time();
        rel < self.tick_len/3
    }


    pub fn sync(&self) Result<()>{
        let e = self.epoch_abs();
        let sl = self.slot_relative();
        self.sl = sl;
        self.e = el;
        Ok(())
    }

    /// absolute zero based slot index
    fn slot_abs(&self) -> i64 {
        let sl_abs = self.tick_abs() / self.sl_len;
        sl_abs.abs()
    }

    /// relative zero based slot index
    fn slot_relative(&self) -> i64 {
        let e_abs = self.slot_abs() % self.e_len;
        e_abs
    }

    /// absolute zero based epoch index.
    fn epoch_abs(&self) -> i64 {
        let res = self.slot_abs() / self.e_len;
        res.abs()
    }

    /// clock ticks return the ticks phase with corresponding phase parameters
    pub fn ticks() -> Ticks {
        let prev_e = self.e;
        let prev_sl = self.sl;
        let e = self.epoch_abs();
        let sl = self.slot_relative();
        if self.ticking() {
            if e==prev_e&&e==BB_E && sl==prev_sl && sl==BB_SL {
                self.sl=sl;
                self.e=e;
                Ticks::GENESIS
            } else if e==prev_e&&sl==prev_sl+1 {
                self.sl=sl;
                Ticks::NEWSLOT{e:e, sl:sl}
            } else if e==prev_e+1 && sl==0 {
                self.e=e;
                self.sl=sl;
                Ticks::NEWEPOCH{e:e, sl:sl}
            }
            else if e==prev_e && sl==prev_sl {
                Ticks::IDLE
            }
            else {
                //clock is out of sync
                Ticks::OUTOFSYNC
            }
        } else {
            Ticks::TOCKS
        }
    }

}
