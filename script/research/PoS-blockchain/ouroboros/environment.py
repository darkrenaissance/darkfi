import numpy as np
import math
import random
'''
\class Z is the environment
'''
class Z(object):
    def __init__(self, stakeholdes, epoch_length=100):
        self.epoch_length=epoch_length
        self.stakeholders = np.array(stakeholdes)
        self.adversary_mask=np.array([True]*len(stakeholdes))
    '''
        return genesis data of the current epoch
    '''
    def get_genesis_data(self):
        #TODO implement    
        pass
    
    @property
    def current_leader_vrf_pk(self):
        #TODO implement
        pass
    
    @property
    def current_leader_vrf_g(self):
        #TODO implement
        pass

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

    def select_epoch_leaders(self, sigma):
        def leader_selection_hash(sigma):
            Y = np.array(sigma)
            y_hypotenuse2 = math.ceil(np.sum(Y[1]**2+Y[2]**2))
            return y_hypotenuse2
        seed = leader_selection_hash(sigma)
        random.seed(seed)
        leader_idx=seed%self.length
        leader = self.stakeholders[leader_idx]
        while not self.adversary_mask[leader_idx]:
            leader_idx=random.randint(0,self.length)
        #TODO select the following leader for this epoch, note, 
        # under a single condition that no one is able to predict who is next
