import numpy as np
import math
import random
import time
from ouroboros.logger import Logger
from ouroboros.consts import *
from ouroboros.data import Item, GenesisItem
from ouroboros import utils

'''
\class Z is the environment,
environment is ought to interfece with the network
'''
class Z(object):
    def __init__(self, stakeholdes, epoch_length, genesis_time=time.time()):
        self.genesis_time=genesis_time
        self.log = Logger(self, genesis_time)
        self.epoch_length=epoch_length
        self.stakeholders = np.array(stakeholdes)
        self.adversary_mask=np.array([True]*len(stakeholdes))
        self.current_epoch_leaders=[-1]*self.epoch_length
        self.current_epoch_endorsers=[-1]*self.epoch_length
        self.current_slot=0
        self.log.info("Z initialized")
        self.current_blk_endorser_sig=None
        self.epoch_inited=False
        self.cached_dist = []
        self.beta = 0.5 # endorser weight
        #
        self.l=0
        #a transaction is declared stable if and only if it is in a block that,
        # is more than k blocks deep in the ledger.
        self.k = self.epoch_length/2 - self.l -1 
    
    @property
    def endorser_len(self):
        #TODO (impl)
        pass

    def __repr__(self):
        buff= f"envirnment of {self.length} stakholders\tcurrent leader's id: {self.current_leader_id}\tepoch_slot: {self.epoch_slot}\tendorser_id: {self.current_endorser_idx}"
        for sh in self.stakeholders:
            buff+=str(sh)+"\n"
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
        return self.current_epoch_leaders[self.epoch_slot]

    @property
    def current_stakeholder(self):
        self.log.info(f"getting leader of id: {self.current_leader_id}")
        return self.stakeholders[self.current_leader_id]

    @property
    def current_endorser_idx(self):
        return self.current_epoch_endorsers[self.epoch_slot]

    def current_endorser_id(self):
        return self.current_endorser.id

    @property
    def current_endorser(self):
        self.log.info(f"getting endorser of id: {self.current_leader_id}")
        return self.stakeholders[self.current_endorser_idx]

    @property
    def current_leader_vrf_pk(self):
        return self.stakeholders[self.current_leader_id].vrf_pk
    
    @property
    def current_leader_vrf_g(self):
        return self.stakeholders[self.current_leader_id].vrf_base

    @property
    def current_leader_sig_pk(self):
        return self.stakeholders[self.current_leader_id].sig_pk
    
    @property
    def current_endorser_sig_pk(self):
        return self.stakeholders[self.current_endorser_idx].sig_pk

    def endorser(self, epoch_slot):
        assert epoch_slot >= 0 and epoch_slot < self.epoch_length
        return self.stakeholders[epoch_slot]
    
    def endorser_sig_pk(self, epoch_slot):
        return self.endorser(epoch_slot).sig_pk

    def endorser_vrf_pk(self, epoch_slot):
        return self.endorser(epoch_slot).vrf_pk

    def leader(self, epoch_slot):
        assert epoch_slot >= 0 and epoch_slot < self.epoch_length
        return self.stakeholders[epoch_slot]

    def leader_sig_pk(self, epoch_slot):
        return self.leader(epoch_slot).sig_pk

    def leader_vrf_pk(self, epoch_slot):
        return self.leader(epoch_slot).vrf_pk
        
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
    def random(self):
        return utils.weighted_random(self.previous_epoch_stake_distribution)

    def select_epoch_leaders(self, sigmas, proofs):
        assert len(sigmas)==self.epoch_length and len(proofs)==self.epoch_length, self.log.error(f"size mismatch between sigmas: {len(sigmas)}, proofs: {len(proofs)}, and epoch_length: {self.epoch_length}")
        for i in range(self.epoch_length):
            self.log.info(f"current sigma of index {i} of total {len(sigmas)}, epoch_length: {self.epoch_length}")
            sigma = sigmas[i]
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
            self.current_epoch_leaders[i]=leader_idx
            self.current_epoch_endorsers[i]=endorser_idx
        return self.current_epoch_leaders, self.current_epoch_endorsers

    def new_slot(self, slot):
        self.current_slot=slot
        self.log.info(f"stakeholders: {self.stakeholders}")
        current_leader = self.stakeholders[self.current_leader_id]
        assert current_leader is not None, "current leader cant be None"
        self.log.highlight('selecting epochs leaders, and ensorsers ---->')
        self.stakeholders[self.current_epoch_endorsers[self.current_endorser_idx]].set_endorser()
        self.stakeholders[self.current_epoch_leaders[self.current_leader_id]].set_leader()
        self.log.highlight('selected epochs leaders, and ensorsers <----')
        
    def new_epoch(self, slot, sigmas, proofs):
        self.cached_dist = self.get_epoch_distribution()
        self.epoch_inited=True
        self.current_slot=slot
        leaders, endorsers = self.select_epoch_leaders(sigmas, proofs)
        return leaders, endorsers

    def broadcast_block(self, signed_block, slot_uid):
        while self.current_blk_endorser_sig is None:
            self.log.info('pending endorsing...')
            time.sleep(1)
            #wait for it untill it gets endorsed
            pass
        for stakeholder in self.stakeholders:
            if not stakeholder.is_leader:
                stakeholder.receive_block(signed_block, self.current_blk_endorser_sig, slot_uid)
        self.print_blockchain()

    @property
    def block_id(self):
        return self.current_slot%self.epoch_length
 
    def endorse_block(self, sig, slot_uid):
        #TODO commit this step to handshake phases
        self.current_blk_endorser_sig=None
        self.log.info(f"endorsing block for current_leader_id: {self.current_leader_id}")
        confirmed = self.stakeholders[self.current_leader_id].confirm_endorsing(sig, self.block_id, self.epoch_slot)
        if confirmed:
            self.current_blk_endorser_sig=sig
        else:
            self.log.warn("unconfirmed endorsed siganture")

    def start(self):
        for sh in self.stakeholders:
            sh(self)
            sh.start()

    def print_blockchain(self):
        for sh in self.stakeholders:
            bc = sh.blockchain
            self.log.highlight(f"<blockchain>  {len(bc)} blocks: "+str(bc))

    def confirm_endorsing(self, sig, blk_uid):
        if blk_uid==self.current_slot:
            self.current_blk_endorser_sig = sig

    def corrupt_leader(self):
        self.corrupt(self.current_leader_id)

    def corrupt_endorse(self):
        self.corrupt(self.current_endorser_idx)

    def corrupt_blk(self):
        self.log.warn(f"<corrupt_blk> at slot: {self.current_slot}")
        self.corrupt_leader()
        self.corrupt_endorse()