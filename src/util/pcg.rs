/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use rand::{CryptoRng, Error, RngCore};

pub struct Pcg32 {
    state: u64,
    increment: u64,
}

impl Pcg32 {
    const MULTIPLIER: u64 = 6364136223846793005;
    const INCREMENT: u64 = 1442695040888963407;

    pub fn new(seed: u64) -> Self {
        let mut rng = Self { state: 0, increment: Self::INCREMENT | 1 };
        rng.state = rng.state.wrapping_add(seed);
        rng.state = rng.state.wrapping_mul(Self::MULTIPLIER).wrapping_add(rng.increment);
        rng
    }

    fn next_u32(&mut self) -> u32 {
        let old_state = self.state;
        self.state = old_state.wrapping_mul(Self::MULTIPLIER).wrapping_add(self.increment);
        let xorshifted = ((old_state >> 18) ^ old_state) >> 27;
        let rot = old_state >> 59;
        ((xorshifted >> rot) | (xorshifted << ((!rot).wrapping_add(1) & 31))) as u32
    }
}

impl CryptoRng for Pcg32 {}

impl RngCore for Pcg32 {
    fn next_u32(&mut self) -> u32 {
        self.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        ((self.next_u32() as u64) << 32) | (self.next_u32() as u64)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut i = 0;
        while i + 4 <= dest.len() {
            let bytes = self.next_u32().to_le_bytes();
            dest[i..i + 4].copy_from_slice(&bytes);
            i += 4;
        }
        if i < dest.len() {
            let bytes = self.next_u32().to_le_bytes();
            for (j, dest_byte) in dest[i..].iter_mut().enumerate() {
                *dest_byte = bytes[j];
            }
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcg() {
        const ITERS: usize = 10000;

        let mut rng0 = Pcg32::new(42);
        let mut rng1 = Pcg32::new(42);

        for i in 0..ITERS {
            let a = rng0.next_u32();
            let b = rng1.next_u32();
            assert!(a == b);

            let a = rng0.next_u64();
            let b = rng1.next_u64();
            assert!(a == b);

            let mut buf0 = vec![0u8; i];
            let mut buf1 = vec![0u8; i];
            rng0.fill_bytes(&mut buf0);
            rng1.fill_bytes(&mut buf1);
            assert!(buf0 == buf1);
        }
    }
}
