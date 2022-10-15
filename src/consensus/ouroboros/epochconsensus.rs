/// epoch configuration
/// this struct need be a singleton,
/// TODO should be populated from configuration file.
#[derive(Copy, Debug, Default, Clone)]
pub struct EpochConsensus {
    pub sl_len: u64, // length of slot in terms of ticks
    // number of slots per epoch
    pub e_len: u64, // length of epoch in terms of slots
    pub tick_len: u64, // length of tick in terms of seconds
    pub reward: u64, // constant reward value for the slot leader
}

impl EpochConsensus {
    pub fn new(
        sl_len: Option<u64>,
        e_len: Option<u64>,
        tick_len: Option<u64>,
        reward: Option<u64>,
    ) -> Self {
        Self {
            sl_len: sl_len.unwrap_or(22),
            e_len: e_len.unwrap_or(3),
            tick_len: tick_len.unwrap_or(22),
            reward: reward.unwrap_or(1),
        }
    }

    pub fn total_stake(&self, e: u64, sl: u64) -> u64 {
        (e*self.e_len +  sl+ 1) * self.reward
    }
    /// getter for constant stakeholder reward
    /// used for configuring the stakeholder reward value
    pub fn get_reward(&self) -> u64 {
        self.reward
    }

    /// getter for the slot length in terms of ticks
    pub fn get_slot_len(&self) -> u64 {
        self.sl_len
    }

    /// getter for the epoch length in terms of slots
    pub fn get_epoch_len(&self) -> u64 {
        self.e_len
    }

    /// getter for the ticks length in terms of seconds
    pub fn get_tick_len(&self) -> u64 {
        self.tick_len
    }
}
