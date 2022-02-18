import threading
from ouroboros.clock import SynchedNTPClock
from ouroboros.vrf import VRF
from ouroboros.logger import Logger

'''
\class TrustedBeacon

the trusted beacon is decentralized, such that at the onset of the Epoch,
the leader of the first slot generated the signed seed, and release the signature, 
proof, and base to the ge nesis block.

#TODO implement trustedbeacon as a node
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
    '''
    def verify(self, y, pi, pk_raw, g):
        return VRF.verify(self.current_slot, y, pi, pk_raw, g)
    '''