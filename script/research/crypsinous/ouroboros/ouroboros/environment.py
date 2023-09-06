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

import numpy as np
import math
import random
import time
from threading import Thread
from ouroboros.logger import Logger
from ouroboros.consts import *
from ouroboros.data import GenesisItem, Data
from ouroboros import utils
from ouroboros.beacon import TrustedBeacon
from ouroboros.block import GensisBlock
from ouroboros.epoch import Epoch
from ouroboros.stakeholder import Stakeholder

'''
\class Z is the environment,
environment is ought to interfece with the network
'''
class Z(object):
    def __init__(self, stakeholdes,  epoch_length, genesis_time=time.time(), f=0.1):
        self.genesis_time=genesis_time
        self.beacon = TrustedBeacon(epoch_length, genesis_time)
        self.log = Logger(self, genesis_time)
        self.epoch_length=epoch_length
        self.stakeholders = np.array(stakeholdes)
        self.adversary_mask=np.array([True]*len(stakeholdes))
        self.slot_committee = {}
        self.current_slot=0
        self.current_blk_endorser_sig=None
        self.epoch_inited=False
        self.cached_dist = []
        self.beta = 0.5 # endorser weight
        #
        self.l=0
        #a transaction is declared stable if and only if it is in a block that,
        # is more than k blocks deep in the ledger.
        self.k = self.epoch_length/2 - self.l -1 
        self.epoch_initialized = {}
        #TODO (fix) replace those by query from blockchain genesis block
        self.rands = {}
        self.prev_leader_id=-1
        #
        self.current_block=None
        self.f = f
        self.init()

    '''
    active slot coefficient  determine the relation between,
    probability of leader being selected, and the relative stake,
    it's defined as the probability that a party holding all the stake
    will be selected to be a leader for given slot.
    '''
    @property
    def slot_coef(self):
        return self.f


    def init(self):
        for sh in self.stakeholders:
            sh(self)
        assert len(self.stakeholders) > 2
        #pick initial leader to be the first stakeholder
        initial_leader = self.stakeholders[0]
        #pick initial endorser to be the first endorser
        #initial_endorser = self.stakeholders[1]
        self.current_epoch = self.beacon.epoch
        self.rands = self.beacon.next_epoch_seeds(initial_leader.vrf)
        self.log.info(f"rands length: {len(self.rands)}")
        self.current_slot = self.beacon.slot
        self.select_epoch_leaders()
        self.prev_leader_id=0
        self.new_epoch_signal()
        #TODO need to assign the block from the last slot in the epoch
        while True:
            if not self.beacon.slot == self.current_slot:
                self.current_slot = self.beacon.slot
                if self.beacon.epoch_slot != 0:
                    self.new_slot_signal()
                else:
                    self.new_epoch_signal()
        
    def new_slot_signal(self):
        ############
        # NEW SLOT #
        ############
        y, pi = self.rands[self.current_slot]
        threads = []
        for sk in self.stakeholders:
            #TODO (fix) failed to synchronized current_slot 1234 for epoch length of 2 is two states
            # need to pass the slot with it's corresponding sigma, and proof
            thread = Thread(target=Stakeholder.new_slot, args=(sk, self.current_slot, y, pi))                
            #sk.new_slot(self.current_slot, y, pi)
            threads.append(thread)
            thread.start()
        for th in threads:
            th.join()

    def new_epoch_signal(self):
        #############
        # NEW EPOCH #
        #############
        vrf = self.stakeholders[self.current_leader_id].vrf
        if self.beacon.epoch != self.current_epoch:
            self.current_epoch = self.beacon.epoch
            self.rands = self.beacon.next_epoch_seeds(vrf)
            self.select_epoch_leaders()
        for idx, sk in enumerate(self.stakeholders):
            if sk.id==id:
                self.prev_leader_id=idx
        self.cached_dist = self.get_epoch_distribution()
        for sk in self.stakeholders:
            sk.current_slot_uid=self.beacon.slot
            ###
        genesis_item = self.get_genesis_data()
        data = Data()
        data.append(genesis_item)
        self.current_block=GensisBlock(self.current_block, data, self.beacon.slot, self.genesis_time)
        assert self.current_block is not None
        current_epoch=Epoch(self.current_block, self.epoch_length, self.epoch, self.genesis_time)
        threads = []
        for sk in self.stakeholders:
            #sk.new_epoch(current_epoch)
            thread = Thread(target=Stakeholder.new_epoch, args=(sk, current_epoch))
            threads.append(thread)
            thread.start()
        for th in threads:
            th.join()


    def __repr__(self):
        buff = ''
        if len (self.slot_committee)>0:
            buff = f"envirnment of {self.length} stakholders\tcurrent leader's id: \
                {self.current_leader_id}\tepoch_slot: {self.epoch_slot}\tendorser_id: \
                     {self.current_endorser_id}"
            for sh in self.stakeholders:
                buff+=str(sh)+"\n"
        else: 
            buff =  f"envirnment of {self.length} stakholders\tepoch_slot: {self.epoch_slot}"
        return buff
   
    '''
    issue a coinbase for claimed reward 2(k+l) after the block
    '''
    def issue_coinbase(self):
        #TODO
        pass

    '''
    returns true if before time, false otherwise
    '''
    @property
    def iceage(self):
        return self.block_id==0

    @property
    def previous_epoch_stake_distribution(self):
        if self.iceage:
            return self.get_epoch_distribution()
        else:
            return self.cached_dist

    def get_epoch_distribution(self):
        stakes = [node.stake for node in self.stakeholders]
        return stakes

    '''
        return genesis data of the current epoch
    '''
    def get_genesis_data(self):
        #TODO implement dynaming staking
        distribution = self.get_epoch_distribution()
        genesis_data = {STAKEHOLDERS: self.stakeholders,
        STAKEHOLDERS_DISTRIBUTIONS: distribution,
            SEED: ''}
        return GenesisItem(genesis_data)
    
    @property
    def epoch_slot(self):
        return self.current_slot%self.epoch_length

    @property
    def current_leader_id(self):
        return self.slot_committee[self.epoch_slot][0]

    @property
    def current_stakeholder(self):
        self.log.info(f"getting leader of id: {self.current_leader_id}")
        return self.stakeholders[self.current_leader_id]

    @property
    def current_endorser_id(self):
        return self.slot_committee[self.epoch_slot][1]

    @property
    def current_endorser_uid(self):
        return self.stakeholders[self.current_endorser_id].id
    
    @property
    def current_endorser(self):
        self.log.info(f"getting endorser of id: {self.current_leader_id}")
        return self.stakeholders[self.current_endorser_id]

    @property
    def current_endorser_sig_pk(self):
        return self.stakeholders[self.current_endorser_id].sig_pk

    def endorser(self, slot):
        return self.stakeholders[self.slot_committee[slot][1]]
    
    def endorser_sig_pk(self, slot):
        return self.endorser(slot).sig_pk

    def endorser_vrf_pk(self, slot):
        return self.endorser(slot).vrf_pk

    def is_current_endorser(self, id):
        _, edr_idx = self.slot_committee[self.epoch_slot]
        return id == self.stakeholders[edr_idx].id

    @property
    def current_leader_vrf_pk(self):
        return self.stakeholders[self.current_leader_id].vrf_pk
    
    @property
    def current_leader_vrf_g(self):
        return self.stakeholders[self.current_leader_id].vrf_base

    def is_current_leader(self, id):
        ldr_idx, _ = self.slot_committee[self.epoch_slot]
        return id == self.stakeholders[ldr_idx].id

    
    '''
    @property
    def current_epoch_leader(self):
        return self.stakeholders[self.current_epoch_leaders[0]]

    @property 
    def current_epoch_leader_vrf_pk(self):
        return self.current_epoch_leader.vrf_pk

    @property
    def current_epoch_leader_vrf_g(self):
        return self.current_epoch_leader.vrf_base
    '''
    @property
    def current_leader_sig_pk(self):
        return self.stakeholders[self.current_leader_id].sig_pk
    
    #note! assumes epoch_slot lays in the current epoch
    def leader(self, slot):
        return self.stakeholders[self.slot_committee[slot][0]]

    '''
    def leader_sig_pk(self, epoch_slot):
        return self.leader(epoch_slot).sig_pk

    def leader_vrf_pk(self, epoch_slot):
        return self.leader(epoch_slot).vrf_pk
    
    def leader_vrf_g(self, epoch_slot):
        return self.leader(epoch_slot).vrf_base
    '''
    def prev_leader_vrf_pk(self):
        return self.stakeholders[self.prev_leader_id].vrf_pk
    
    def prev_leader_vrf_g(self):
        return self.stakeholders[self.prev_leader_id].vrf_base

    #TODO complete
    def obfuscate_idx(self, i):
        return i

    #TODO complete
    def deobfuscate_idx(self, i):
        return i

    def corrupt(self, i):
        if i<0 or i>len(self.adversary_mask):
            return False
        self.adversary_mask[self.deobfuscate_idx(i)]=False
        return True
    
    '''
    return the length of all parties
    '''
    def __len__(self):
        return len(self.stakeholders)

    @property
    def length(self):
        return len(self.stakeholders)

    @property
    def honest(self):
        return len(self.stakeholders[self.adversary_mask])

    @property
    def epoch_stake_distribution(self):
        #stakes = {}
        ordered_stakes = [] #with the same stakeholders order
        for sk in self.stakeholders:
            #stakes[sk.id] = sk.stake
            ordered_stakes.append(sk.stake)
        return  ordered_stakes

    @property
    def random(self):
        return utils.weighted_random(self.epoch_stake_distribution)

    '''
    since clocks are synched
    '''
    @property
    def epoch(self):
        return self.beacon.epoch

    def select_epoch_leaders(self):
        self.log.info("[Z/select_epoch_leaders]")
        #assert len(self.sigmas)==self.epoch_length and len(self.proofs)==self.epoch_length, \
            #self.log.error(f"size mismatch between sigmas: {len(self.sigmas)}, proofs: {len(self.proofs)}, and epoch_length: {self.epoch_length}")
        for i in range(self.epoch_length):
            self.log.info(f"current sigma of index {i} , epoch_length: {self.epoch_length}, rand : {self.rands}")
            slot_idx = self.current_slot + i
            assert len(self.rands)>0
            slot_idx_relative = slot_idx%self.epoch_length
            sigma, _ = self.rands[slot_idx]
            assert sigma!=None, 'proof cant be None'
            def leader_selection_hash(sigma):
                Y = np.array(sigma)
                y_hypotenuse2 = math.ceil(np.sum(Y[1]**2+Y[2]**2))
                return y_hypotenuse2
            seed = leader_selection_hash(sigma)
            random.seed(seed)
            leader_idx=self.random
            endorser_idx=self.random
            # only select an honest leaders
            while leader_idx==endorser_idx or not self.adversary_mask[leader_idx] or not self.adversary_mask[endorser_idx]:
                leader_idx=self.random
                endorser_idx=self.random
            #TODO select the following leader for this epoch, note, 
            # under a single condition that no one is able to predict who is next
            assert not leader_idx==endorser_idx
            #TODO move leader/endorser to a dictionary
            self.slot_committee[slot_idx_relative] = (leader_idx, endorser_idx)
            self.log.highlight(f'slot {slot_idx} has committee leader/endorser {leader_idx}/{endorser_idx}\nleader: {self.stakeholders[leader_idx]}\nendorser: {self.stakeholders[endorser_idx]}')
        self.epoch_initialized[str(self.epoch)] = True

    def broadcast_block(self, cur_block):
        while self.current_blk_endorser_sig is None:
            self.log.info('pending endorsing...')
            time.sleep(1)
            #wait for it untill it gets endorsed
            pass
        self.current_block = cur_block
        for stakeholder in self.stakeholders:
            if not stakeholder.is_leader:
                stakeholder.receive_block(cur_block, self.current_blk_endorser_sig)
                #stakeholder.receive_block(cur_block.signature, self.current_blk_endorser_sig, cur_block.slot)
        self.print_blockchain()

    @property
    def block_id(self):
        return self.current_slot%self.epoch_length
 
    def endorse_block(self, sig, slot_uid):
        #TODO commit this step to handshake phases
        self.current_blk_endorser_sig=None
        self.log.info(f"endorsing block for current_leader_id: {self.current_leader_id}")
        confirmed = self.stakeholders[self.current_leader_id].confirm_endorsing(sig, self.block_id, self.current_slot)
        if confirmed:
            self.current_blk_endorser_sig=sig
        else:
            self.log.warn("unconfirmed endorsed siganture")


    def print_blockchain(self):
        for sh in self.stakeholders:
            bc = sh.blockchain
            self.log.highlight(f"<blockchain>  {len(bc)} blocks: "+str(bc))

    '''
    def confirm_endorsing(self, sig, blk_uid):
        if blk_uid==self.current_slot:
            self.current_blk_endorser_sig = sig
    '''
    def corrupt_leader(self):
        self.corrupt(self.current_leader_id)

    def corrupt_endorser(self):
        self.corrupt(self.current_endorser_id)

    def corrupt_blk(self):
        self.log.warn(f"<corrupt_blk> at slot: {self.current_slot}")
        self.corrupt_leader()
        self.corrupt_endorser()