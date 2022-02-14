from copy import deepcopy
import time
from ouroboros.block import Block, GensisBlock, EmptyBlock
from ouroboros.blockchain import Blockchain
from ouroboros.epoch import Epoch
from ouroboros.beacon import TrustedBeacon
from ouroboros.vrf import verify, VRF
from ouroboros.utils import *
from ouroboros.logger import Logger
from ouroboros.consts import *

'''
\class Stakeholder
'''
class Stakeholder(object):
    def __init__(self, epoch_length=10, passwd='password'):
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
        self.am_current_endorser=False
        self.am_corrupt=False
        #
        self.blockchain=None

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
        buff=''
        if self.am_current_leader:
            buff = f"\tleader {(hash(self.passwd))} with stake:{self.stake}\nsig_sk: {self.sig_pk}"
        elif self.am_current_endorser:
            buff = f"\tendorser {(hash(self.passwd))} with stake:{self.stake}\nsig_sk: {self.sig_pk}"
        else:
            buff = f"\thonest committee memeber {(hash(self.passwd))} with stake:{self.stake}\nsig_sk: {self.sig_pk}"
        return buff

    def __call__(self, env):
        self.env=env
        self.log = Logger(self, self.env.genesis_time)
        self.blockchain = Blockchain(self.epoch_length, self.env.genesis_time)
        self.beacon = TrustedBeacon(self,  self.vrf, self.epoch_length, self.env.genesis_time)
        self.current_slot_uid = self.beacon.slot


    def start(self):
        self.log.info("thread [started]")
        self.beacon.start()
        self.log.info("thread [ended]")

    @property
    def epoch_index(self):
        return round(self.current_slot_uid/self.epoch_length)

    def __gen_genesis_epoch(self):
        '''
        '''
        self.tx = self.env.get_genesis_data()
        self.tx[TX]=self.uncommited_tx
        self.uncommited_tx=''
        self.current_block=GensisBlock(self.current_block, self.tx, self.current_slot_uid, self.env.genesis_time)
        assert self.current_block is not None
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
            self.end_leadership()
        elif self.am_current_endorser:
            assert self.current_slot_uid==self.env.current_slot, f' current slot: {self.current_slot_uid}, env current slot {self.env.current_slot}'
            #assert self.sig_pk==self.env.current_endorser_sig_pk, f'current sig_pk: {self.sig_pk}\nZ sig_pk:{self.env.current_endorser_sig_pk}'
            if not self.sig_pk==self.env.current_endorser_sig_pk:
                return
            self.endorse_block()
            self.end_endorsing()

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
        if not verify(slot, sigma, proof, vrf_pk,vrf_g) :
            #TODO the leader is corrupted, action to be taken against the corrupt stakeholder
            #in this case this slot is empty
            self.log.warn(f"<new_slot> leader verification fails")
            self.current_block=EmptyBlock(self.env.genesis_time) 
            if self.current_epoch==None:
                self.__gen_genesis_epoch()
            self.current_epoch.add_block(self.current_block)
            return
        if self.current_epoch==None:
            self.log.warn(f"<new_slot> current_epoch is None!")
            self.__gen_genesis_epoch()
        self.current_slot_uid = slot
        prev_blk = self.blockchain[-1] if len(self.blockchain)>0 else EmptyBlock(self.env.genesis_time)
        self.current_block=Block(prev_blk, self.tx, self.current_slot_uid, self.env.genesis_time)
        self.current_epoch.add_block(self.current_block)
        assert self.current_block is not None
        if self.am_current_leader:
            self.log.highlight(f"leader {str(self)} is broadcasting block")
            self.broadcast_block()
            self.end_leadership()
        elif self.am_current_endorser:
            self.log.highlight(f"endorser {str(self)} is endorsing block")
            assert self.sig_pk==self.env.current_endorser_sig_pk
            self.endorse_block()
            self.end_endorsing()
        else:
            self.log.highlight(f"committee memeber is listening...")


    def end_leadership(self):
        self.log.info(f"stakeholder:{str(self)} ending leadership for slot{self.current_slot_uid}")
        self.am_current_leader=False

    def end_endorsing(self):
        self.log.info(f"stakeholder:{str(self)} ending endorsing for slot{self.current_slot_uid}")
        self.am_current_endorser=False

    def set_leader(self):
        self.am_current_leader=True

    def set_endorser(self):
        self.am_current_endorser=True

    def set_corrupt(self):
        self.am_corrupt=False

    def broadcast_block(self):
        self.log.highlight("broadcasting block")
        assert self.am_current_leader and self.current_block is not None
        signed_block=None
        #TODO should wait for l slot until block is endorsed
        endorsing_cnt=10
        while not self.current_block.endorsed:
            time.sleep(1)
            self.log.info("...waiting for endorsment..")
            endorsing_cnt-=1
        if not self.current_block.endorsed:
            self.log.warn("failure endorsing the block...")
        if not self.current_block.endorsed:
            self.current_block = EmptyBlock(self.env.genesis_time)
        signed_block = sign_message(self.passwd, self.sig_sk, self.current_block)
        self.env.broadcast_block(signed_block, self.current_slot_uid)
    
    def endorse_block(self):
        if not self.am_current_endorser:
            return
        self.log.info(f"endorsing block for current_leader_id: {self.env.current_leader_id}")
        if not self.am_current_endorser:
            self.log.warn("not endorser")
            return
        assert self.current_block is not None
        sig = sign_message(self.passwd, self.sig_sk, self.current_block)
        self.log.highlight(f'block to be endorsed  {str(self.current_block)}')
        self.log.highlight(f'block to be endorsed has slot_uid: {self.current_slot_uid}')
        self.log.highlight(f'block to be endorsed has sig_pk: {str(self.sig_pk)}')
        assert self.env.current_endorser_sig_pk==self.sig_pk
        self.env.endorse_block(sig, self.current_slot_uid)

    def __get_blk(self, blk_uid):
        assert(blk_uid>=0)
        stashed=True
        cur_blk = self.current_block
        if blk_uid < len(self.blockchain):
            #TODO this assumes synced blockchain
            cur_blk = self.blockchain[blk_uid]
            self.log.warn(f"current block from blockchain: {(cur_blk)}")
            stashed=False
        self.log.info(f"current block : {str(cur_blk)}\tblock uid: {blk_uid}\tstashed: {stashed}")
        if cur_blk is None:
            self.log.warn(f'block is none, current block is {str(self.current_block)} and current slot {self.current_slot_uid}, current block uid {blk_uid}, env slot {self.env.current_slot}, env blk {self.env.block_id}')
        assert cur_blk is not None and self.current_block is not None
        return cur_blk, stashed

    def receive_block(self, signed_block, endorser_sig, blk_uid):
        self.log.highlight("receiving block")
        cur_blk, stashed = self.__get_blk(blk_uid)
        assert cur_blk is not None
        #TODO to consider deley should retrive leader_pk of corresponding blk_uid
        self.log.highlight(f'receiving block  {str(cur_blk)}')
        self.log.highlight(f'receiving block has slot_uid: {self.current_slot_uid}')
        self.log.highlight(f'receiving block has sig_pk: {self.env.current_endorser_sig_pk}')
        blk_verified = verify_signature(self.env.current_leader_sig_pk, cur_blk, signed_block)
        self.log.info("endorser sig_pk {self.env.current_endorser_sig_pk}, cur_blk: {cur_blk}, endorser_sig: {endorser_sig}")
        blk_edrs_verified = verify_signature(self.env.current_endorser_sig_pk, cur_blk, endorser_sig)
        if blk_verified and blk_edrs_verified:
            if stashed:
                self.current_epoch.add_block(cur_blk)
        else:
            if not blk_verified:
                self.log.warn("block verification failed")
            elif not blk_edrs_verified:
                self.log.warn("block endorsing verification failed")
            self.env.corrupt_blk()

    def confirm_endorsing(self, endorser_sig, blk_uid, epoch_slot):
        while not self.current_slot_uid == self.env.current_slot:
            self.log.info(" ...pending start of slot...")
            time.sleep(1)
        self.log.highlight("receiving block")
        confirmed = False
        cur_blk, _ = self.__get_blk(blk_uid)
        assert cur_blk is not None
        self.log.highlight(f'confirming endorsed block  {str(cur_blk)}')
        self.log.highlight(f'confirming endorsed has slot_uid: {self.current_slot_uid}')
        self.log.highlight(f'confirming endorsed has sig_pk: {self.env.current_endorser_sig_pk}')
        if verify_signature(self.env.current_endorser_sig_pk, cur_blk, endorser_sig):
            if self.current_slot_uid==self.env.current_slot:
                self.current_block.set_endorsed()
            else:
                self.blockchain[blk_uid].set_endorsed()
            confirmed=True
        else:
            self.log.warn(f"confirmed enderser signature failure for pk: {str(self.env.current_endorser_sig_pk)} on block {str(cur_blk)}  of signature {str(endorser_sig)}")
            confirmed=False
        return confirmed
