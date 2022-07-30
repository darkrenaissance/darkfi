use url::Url;
use log::debug;
use async_trait::async_trait;
use crate::{
    util::{time,Timestamp},
    error,
    Result,
    error::Error
};

pub enum Ticks {
    GENESIS{e: u64, sl: u64}, //genesis epoch
    NEWSLOT{e: u64, sl: u64}, // new slot
    NEWEPOCH{e: u64, sl: u64}, // new epoch
    TOCKS, //tocks, or slot is ending
    IDLE, // idle clock state
    OUTOFSYNC, //clock, and blockchain are out of sync
}

const BB_SL : u64 = u64::MAX; //big bang slot time (need to be negative value)
const BB_E : u64 = 0; //big bang epoch time.
const GENESIS_TIME : i64 = 0;

#[derive(Debug)]
pub struct Clock {
    pub sl : u64, // relative slot index (zero-based) [0-len[
    pub e : u64, //epoch index (zero-based) [0-\inf[
    pub tick_len: u64, // tick length in time
    pub sl_len: u64, // slot length in ticks
    pub e_len: u64, // epoch length in slots
    pub peers: Vec<Url>,
}

impl Clock {
    pub fn new(e_len: Option<u64>, sl_len: Option<u64>, tick_len: Option<u64>, peers: Vec<Url>) -> Self{
        Self { sl: BB_SL, //necessary for genesis slot
               e: BB_E,
               tick_len: tick_len.unwrap_or(22), // 22 seconds
               sl_len: sl_len.unwrap_or(22),// ~8 minutes
               e_len: e_len.unwrap_or(3), // 24.2 minutes
               peers: peers,
        }
    }

    pub fn get_sl_len(&self) -> u64 {
        self.sl_len
    }

    pub fn get_e_len(&self) -> u64 {
        self.e_len
    }

    async fn time(&self) -> Result<Timestamp> {
        match time::check_clock(self.peers.clone()).await {
            Ok(t) => {
                Ok(time::ntp_request().await?)
            },
            Err(e) => {
                Err(Error::ClockOutOfSync(e.to_string()
))
            }
        }
    }

    /// time since genesis
    async fn time_to_genesis(&self) -> Timestamp {
        let genesis_time : i64 = GENESIS_TIME;
        let abs_time = self.time().await.unwrap();
        Timestamp(abs_time.0 - genesis_time)
    }

    async fn tick_time(&self) -> (u64, u64) {
        let time = self.time_to_genesis().await;
        let time_i = time.0 as u64;
        let tick_abs: u64 = time_i / self.tick_len;
        let tick_rel: u64 = time_i % self.tick_len;
        (tick_abs, tick_rel)
    }

    /// return true if the clock is at the begining (before 2/3 of the slot).
    async fn ticking(&self) -> bool {
        let (abs, rel) =  self.tick_time().await;
        rel < (self.tick_len) /3
    }

    pub async fn sync(& mut self) -> Result<()> {
        let e = self.epoch_abs().await;
        let sl = self.slot_relative().await;
        self.sl = sl;
        self.e = e;
        Ok(())
    }

    /// absolute zero based slot index
    async fn slot_abs(&self) -> u64 {
        let sl_abs = self.tick_time().await.0 / self.sl_len;
        sl_abs
    }

    /// relative zero based slot index
    async fn  slot_relative(&self) -> u64 {
        let e_abs = self.slot_abs().await % self.e_len;
        e_abs
    }

    /// absolute zero based epoch index.
    async fn epoch_abs(&self) -> u64 {
        let res = self.slot_abs().await / self.e_len;
        res
    }

    /// clock ticks return the ticks phase with corresponding phase parameters
    pub async fn ticks(&mut self) -> Ticks {
        let prev_e = self.e;
        let prev_sl = self.sl;
        let e = self.epoch_abs().await;
        let sl = self.slot_relative().await;
        if self.ticking().await {
            if e==prev_e&&e==BB_E && sl==prev_sl && sl==BB_SL {
                self.sl=sl; // 0
                self.e=e; // 0
                Ticks::GENESIS{e:e, sl:sl}
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
