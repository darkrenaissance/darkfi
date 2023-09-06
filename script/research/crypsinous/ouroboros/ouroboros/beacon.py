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

import threading
from ouroboros.clock import SynchedNTPClock
from ouroboros.vrf import VRF
from ouroboros.logger import Logger

'''
\class TrustedBeacon

leaky, non-resettable beacon, leaky in the sense that the slots are 
predictable, and non-resettable, beacon is basically a synchronized 
timestamp.
'''
class TrustedBeacon(SynchedNTPClock, threading.Thread):

    def __init__(self, epoch_length, genesis_time):
        SynchedNTPClock.__init__(self, epoch_length)
        threading.Thread.__init__(self)
        self.daemon=True
        self.current_slot = self.slot
        self.log = Logger(self, genesis_time)
        self.bb=0 # epoch counts since genesis (big bang)
        self.proofs_epoch=-1

    def __repr__(self):
        return f"trustedbeacon"

    def run(self):
        self.log.highlight("thread [start]")
        prev_slot = self.slot
        self.__callback()
        while True:
            if not self.slot == prev_slot:
                prev_slot = self.slot
                self.__callback()
           
    def next_epoch_seeds(self, vrf):
        rands = {}
        for i in range(self.epoch_length):
            slot_idx = self.current_slot+i
            y, pi = vrf.sign(slot_idx)
            rands[slot_idx] = (y,pi)
        return rands
