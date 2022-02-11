#from asyncio.log import logger
from ouroboros.block import Block, GensisBlock, EmptyBlock
from ouroboros.blockchain import Blockchain
from ouroboros.epoch import Epoch
from ouroboros.beacon import TrustedBeacon
from ouroboros.vrf import verify, VRF
from ouroboros.utils import *
from ouroboros.logger import Logger
from ouroboros.consts import *
import time
'''
\class Stakeholder
'''
class Stakeholder(object):
    def __init__(self, epoch_length=2, passwd='password'):
        #TODO (fix) remove redundant variables reley on environment
        self.passwd=passwd
        self.stake=0
        self.epoch_length=epoch_length
        self.vrf = VRF(self.passwd)
        #verification keys
        self.__vrf_pk = self.vrf.pk
        self.__vrf_sk = self.vrf.sk
        self.__vrf_base = self.vrf.g
        #signature keys
        sig_sk, sig_pk = generate_sig_keys(self.passwd)
        self.sig_sk = sig_sk
        self.sig_pk = sig_pk
        #
        self.current_block = None
        self.uncommited_tx=''
        self.tx=''
        self.current_epoch = None
        self.am_current_leader=False
        self.am_current_endorder=False
        self.am_corrupt=False
        #

    @property
    def is_leader(self):
        return self.am_current_leader

    
    @property
    def vrf_pk(self):
        return self.__vrf_pk

    @property
    def vrf_base(self):
        return self.__vrf_base
    
    def __repr__(self):
        buff = f"\tstakeholder with stake:{self.stake}\t"
        return buff

    def __call__(self, env):
        self.env=env
        self.log = Logger(self, self.env.genesis_time)
        self.blockchain = Blockchain(self.epoch_length, self.env.genesis_time)
        self.beacon = TrustedBeacon(self,  self.vrf, self.epoch_length, self.env.genesis_time)
        self.current_slot_uid = self.beacon.slot


    def start(self):
        self.log.info("start [started]")
        self.beacon.start()
        self.log.info("start [ended]")

    @property
    def epoch_index(self):
        return round(self.current_slot_uid/self.epoch_length)

    def __gen_genesis_epoch(self):
        '''
        '''
        self.tx = self.env.get_genesis_data()
        self.tx[TX]=self.uncommited_tx
        self.uncommited_tx=''
        self.current_block=GensisBlock(self.current_block, self.tx, self.current_slot_uid)
        self.current_epoch=Epoch(self.current_block, self.epoch_length, self.epoch_index, self.env.genesis_time)
    '''
    it's a callback function, and called by the diffuser
    '''
    def new_epoch(self, slot, sigmas, proofs):
        '''
        #TODO implement praos
        for this implementation we assume synchrony,
        and at this point, and no delay is considered (for simplicity)
        '''
        self.log.highlight("<new_epoch> start")
        self.env.new_epoch(slot, sigmas, proofs)
        self.current_slot_uid = slot
        #kickoff gensis block
        # add old epoch to the ledger
        if self.current_slot_uid > 1 and self.current_epoch!=None and len(self.current_epoch)>0:
            self.blockchain.add_epoch(self.current_epoch) 
        #if leader, you need to broadcast the block
        self.__gen_genesis_epoch() 
        if self.am_current_leader:
            self.broadcast_block()

    '''
    it's a callback function, and called by the diffuser
    '''
    def new_slot(self, slot, sigma, proof):
        '''
        #TODO implement praos
        for this implementation we assume synchrony,
        and at this point, and no delay is considered (for simplicity)
        '''
        self.log.highlight("<new_slot> start")
        self.env.new_slot(slot, sigma, proof)
        vrf_pk = self.env.current_leader_vrf_pk
        vrf_g = self.env.current_leader_vrf_g
        assert(vrf_pk!=None)
        assert(vrf_g!=None)
        if not verify(slot, sigma, proof, vrf_pk,vrf_g) :
            #TODO the leader is corrupted, action to be taken against the corrupt stakeholder
            #in this case this slot is empty
            self.current_block=EmptyBlock() 
            if self.current_epoch!=None:
                self.current_epoch.add_block(self.current_block)
            else:
                self.log.warn(f"<new_slot> current_epoch is None!")
                return
        if self.current_epoch==None:
            self.log.warn(f"<new_slot> current_epoch is None!")
            self.__gen_genesis_epoch()
        self.current_slot_uid = slot
        self.current_block=Block(self.current_block, self.tx, self.current_slot_uid)
        self.current_epoch.add_block(self.current_block)
        if self.am_current_leader:
            self.broadcast_block()

    def set_leader(self):
        self.am_current_leader=True

    def set_endorser(self):
        self.am_endorser=True

    def set_corrupt(self):
        self.am_corrupt=False

    def broadcast_block(self):
        self.log.highlight("broadcasting block")
        assert(self.am_current_leader)
        signed_block = sign_message(self.passwd, self.sig_sk, self.current_block)
        self.env.broadcast_block(signed_block)
        self.env.print_blockchain()


    def receive_block(self, received_block):
        self.log.highlight("receiving block")
        if verify_signature(self.env.current_leader_sig_pk, self.current_block, received_block):
            pass
        else:
            self.env.corrupt(self.env.current_leader_id)
        self.env.print_blockchain()
