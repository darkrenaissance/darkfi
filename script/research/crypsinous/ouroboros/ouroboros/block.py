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

import json
import time
from ouroboros.utils import encode_genesis_data, decode_gensis_data, state_hash
from ouroboros.consts import *
from ouroboros.logger import Logger

'''
single block B_i for slot i in the system live time L,
assigned to stakeholder U_j, with propability P_j_i,
in the chain C, should be signed by U_j keys.
'''
class Block(object):
    '''
    @param previous_block: parent block to the current block
    @param data: is the transaction, or contracts in the leadger, or gensis block data, 
        data is expected to be binary, no format is enforced
    @param slot_uid: the block corresponding slot monotonically increasing index, 
        it's one-based
    @param gensis: boolean, True for gensis block
    '''
    def __init__(self, previous_block, data, slot_uid, genesis_time=time.time(), genesis=False):
        # state is hte hash of the previous block in the blockchain
        self.state=''
        if slot_uid>1:
            self.state=state_hash(previous_block)
        self.tx = data
        self.sl = slot_uid
        self.signature = None # block issuer signature
        self.sigma = None #  proof that the block is valid.
        self.is_genesis=genesis
        self.endorsed=False
        self.log = Logger(genesis_time)
        self.leader_id=None
        self.endorser_id=None

    @property
    def slot(self):
        return self.sl
    
    def __repr__(self):
        if self.is_genesis:
            return "GensisBlock " + str(self.__to_dict)
        return "Block " + str(self.__to_dict)
    
    @property
    def __to_dict(self):
        return  {SLOT: self.st, STATE: self.state, DATA: self.tx, PROOF: self.sigma, SIGN: self.signature}

    def __hash__(self):
        if type(self.tx)==str:
            return hash((self.state, self.tx, self.sl))
        elif type(self.tx)==dict:
            return hash((self.state, self.tx[SEED], self.tx[TX]))
        else: 
            return hash(str(self))

    def __eq__(self, block):
        return self.state==block.state and \
            self.tx == block.tx and \
            self.sl == block.sl

    def to_json(self):
        d = self.__to_dict
        return json.encoder(d)

    def set_endorsed(self):
        self.endorsed=True

    def set_endorser(self, id):
        self.endorser_id=id
        self.set_endorsed()

    def set_leader(self, id):
        self.leader_id=id
    
    def set_sigma(self, sigma):
        self.sigma = sigma
    
    def set_signature(self, sign):
        self.signature = sign

    @property
    def data(self):
        return self.tx

    @property
    def slot(self):
        return self.sl

    @property
    def empty(self):
        return (self.tx=='' or self.slot<0) and self.state==''

    def encode(self):
        return str(self.__to_dict).encode()

class GensisBlock(Block):
    '''
    @param data: data is dictionary of  list of (pk_i, s_i) public key,
        and stake respectively of the corresponding stakeholder U_i,
        seed of the leader election function.
    '''
    def __init__(self, previous_block, data, slot_uid, genesis_time=time.time()):
        # stakeholders is list of tuple (pk_i, s_i) for the ith stakeholder
        dist_block = data[0]
        self.stakeholders = dist_block[STAKEHOLDERS]
        self.distribution = dist_block[STAKEHOLDERS_DISTRIBUTIONS]
        self.seed = dist_block[SEED] #needed for pvss
        shd_buff = ''
        for shd in self.distribution:
            shd_buff +=str(shd)
        #data = encode_genesis_data(shd_buff)
        data_dict = {'seed':self.seed, 'distribution':shd_buff}
        Block.__init__(self, previous_block, str(data_dict), slot_uid, genesis_time, True)
    '''
    @return: the number of participating stakeholders in the blockchain
    '''
    @property
    def length(self):
        return len(self.stakeholders)

    def __getitem__(self, i):
        if i<0 or i>=self.length:
            raise "index is out of range!"
        return self.stakeholders[i]

'''
block lead by an adversary, or 
lead by offline leader
is an empty Block
'''
class EmptyBlock(Block):
    def __init__(self, genesis_time=time.time()):
        Block.__init__(self, '', -1, genesis_time, False)
