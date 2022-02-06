from block import Block, GensisBlock, EmptyBlock
from blockchain import Blockchain
from epoch import Epoch
from beacon import TrustedBeacon
from vrf import generate_vrf_keys
from utils import *
import numpy as np
import math

'''
\class Stakeholder
'''
class Stakeholder(object):
    def __init__(self, env, epoch_length=100, passwd='password'):
        self.passwd=passwd
        self.epoch_length=epoch_length
        self.blockchain = Blockchain(self.epoch_length)
        self.beacon = TrustedBeacon(self, self.epoch_length)
        self.beacon.start()
        pk, sk, g = generate_vrf_keys(self.passwd)
        self.vrf_pk = pk
        self.vrf_sk = sk
        self.vrf_base = g
        self.current_block = None
        self.uncommited_tx=''
        self.tx=''
        self.current_slot_uid = self.beacon.slot
        self.current_epoch = None
        self.env = env
        
    @property
    def epoch_index(self):
        return round(self.current_slot_uid/self.epoch_length)

    '''
    it's a callback function, and called by the diffuser
    '''
    def new_slot(self, slot, sigma, proof, new_epoch=False):
        '''
        #TODO implement praos
        for this implementation we assume synchrony,
        and at this point, and no delay is considered (for simplicity)
        '''

        if not self.beacon.verify(sigma, proof, self.env.current_leader_vrf_pk, self.env.current_leader_vrf_g):
            #TODO the leader is corrupted, action to be taken against the corrupt stakeholder
            #in this case this slot is empty
            self.current_block=EmptyBlock() 
            self.current_epoch.add_block(self.current_block)
            return
        self.current_slot_uid = slot
        if new_epoch:
            # add epoch to the ledger
            if self.current_slot_uid > 1:
                self.blockchain.add_epoch(self.current_epoch)
            #kickoff gensis block
            self.tx = self.env.get_genesis_data()
            self.current_block=GensisBlock(self.current_block, self.tx, self.current_slot_uid)
            self.current_epoch=Epoch(self.current_block, self.epoch_length, self.epoch_index)
            #TODO elect leaders
            self.select_leader(slot, sigma, proof)

        else:
            self.current_block=Block(self.current_block, self.tx, self.current_slot_uid)
            self.current_epoch.add_block(self.current_block)


    def select_leader(self, slot, sigma, proof):
        #TODO implement
        pass