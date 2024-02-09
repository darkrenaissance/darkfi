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

from ouroboros.utils import state_hash
from ouroboros.logger import Logger

class Epoch(list):
    '''
    epoch spans R slots, 
    maximum number of block in Epoch is R
    epoch must start with gensis block B0
    '''
    def __init__(self, gensis_block, R, epoch_idx, genesis_time):
        self.gensis_block=gensis_block
        self.blocks = []
        self.R = R #maximum epoch legnth, and it's a fixed property of the system
        self.e = epoch_idx
        self.log = Logger(genesis_time)

    @property
    def slot(self):
        return self.gensis_block.sl
    
    @property
    def length(self):
        return len(self.blocks)

    @property
    def index(self):
        return self.e

    @property
    def genesis(self):
        if self.length==0:
            return None
        return self.blocks[0]

    @property
    def coffee(self):
        epoch_fee = 0
        for blk in self.blocks:
            epoch_fee += blk.data.coffee
        return epoch_fee

    def __len__(self):
        return self.length

    def add_block(self, block):
        if len(self.blocks)>0 and not block.state==state_hash(self.blocks[-1]):
            #TODO we dealing with corrupt stakeholder,
            # action to be taken
            # the caller of the add_block should execute (corrupt,U)
            pass

        if self.length>=self.R:
            self.log.error(f"epoch length: {self.length} can't exceed Epoch's length: {self.R}")
        self.blocks.append(block)
    
    def __iter__(self):
        self.n=0
        return self
    
    def __next__(self):
        blk=None
        if self.n <= self.length:
            try:
                blk=self.blocks[self.n]
                self.n+=1
                return blk
            except IndexError:
                raise StopIteration
