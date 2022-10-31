/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use crate::{
    consensus::{coins, ouroboros::EpochConsensus},
    crypto::{
        coin::OwnCoin,
        lead_proof,
        leadcoin::LeadCoin,
        proof::{Proof, ProvingKey},
    },
};
use log::info;
use pasta_curves::pallas;

#[derive(Debug, Default, Clone)]
pub struct Epoch {
    pub consensus: EpochConsensus,
    // should have ep, slot, current block, etc.
    pub eta: pallas::Base,     // CRS for the leader selection.
    coins: Vec<Vec<LeadCoin>>, // competing coins
}

impl Epoch {
    pub fn new(consensus: EpochConsensus, true_random: pallas::Base) -> Self {
        Self { consensus, eta: true_random, coins: vec![] }
    }

    /// retrive leadership lottary coins of static stake,
    /// retrived for for commitment in the genesis data
    pub fn get_coins(&self) -> Vec<Vec<LeadCoin>> {
        self.coins.clone()
    }

    pub fn get_coin(&self, sl: usize, idx: usize) -> LeadCoin {
        self.coins[sl][idx]
    }

    pub fn len(&self) -> usize {
        self.consensus.get_epoch_len() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn col(&self) -> usize {
        if self.coins.is_empty() {
            0
        } else {
            self.coins[0].len()
        }
    }

    /// Wrapper for coins::create_epoch_coins
    pub fn create_coins(&mut self, e: u64, sl: u64, owned: &Vec<OwnCoin>) {
        self.coins = coins::create_epoch_coins(self.eta, owned, e, sl);
    }

    /// Wrapper for coins::is_leader
    pub fn is_leader(&self, sl: u64) -> (bool, usize) {
        coins::is_leader(sl, &self.coins)
    }

    /// * `sl` - relative slot index (zero based)
    /// * `idx` - idex of the highest winning coin
    /// * `pk` - proving key
    /// returns  the of proof of the winning coin of slot `sl` at index `idx` with
    /// proving key `pk`
    pub fn get_proof(&self, sl: u64, idx: usize, pk: &ProvingKey) -> Proof {
        info!("get_proof");
        let competing_coins: &Vec<LeadCoin> = &self.coins.clone()[sl as usize];
        let coin = competing_coins[idx];
        lead_proof::create_lead_proof(pk, coin).unwrap()
    }
}
