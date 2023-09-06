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

import math
from ouroboros.logger import Logger

'''
Non-forkable Blockchain for simplicity
#TODO implement forkable chain
'''
class Blockchain(object):
    def __init__(self, R, genesis_time):
        self.blocks = []
        self.log = Logger(self, genesis_time)
        self.R = R # how many slots in single epoch
        self.epochs = []
    
    @property
    def epoch_length(self):
        return self.R

    def __repr__(self):
        buff=''
        for e in range(len(self.epochs)):
            for b in range(len(e)):
                buff+=str(b)+'\n'
        return buff

    '''
    @return: epoch reference
    '''
    def __getitem__(self, i):
        e_idx = math.floor(i/self.epoch_length)
        e_blk_idx = i%self.epoch_length
        return self.epochs[e_idx][e_blk_idx]

    '''
    @return: number of blocks
    '''
    def __len__(self):
        return len(self.epochs*self.epoch_length)

    '''
    def __add_block(self, block):
        self.blocks.append(block)
    
    def add_epoch(self, epoch):
        assert epoch!=None, 'epoch cant be None'
        assert len(epoch)>0 , 'epoch cant be zero-sized'
        for idx, block in enumerate(epoch):
            if not block.empty:
                self.__add_block(block)
            else:
                self.log.warn(f"an empty block at index of index: {block.index},\nrelative slot:{idx}\nabsolute slot: {self.length*idx+block.slot}")
    '''
    def append(self, epoch):
        assert epoch!=None, 'epoch cant be None'
        assert len(epoch)>0 , 'epoch cant be zero-sized'
        assert len(epoch)==self.epoch_length
        self.append(epoch)