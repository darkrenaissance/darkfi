from clock import SynchedNTPClock
from vrf import VRF
import threading
import time 

'''
\class TrustedBeacon

the trusted beacon is decentralized, such that at the onset of the Epoch,
the leader of the first slot generated the signed seed, and release the signature, 
proof, and base to the genesis block.

#TODO implement trustedbeacon as a node
'''
class TrustedBeacon(SynchedNTPClock, threading.Thread):
    def __init__(self, node, epoch_length):
        SynchedNTPClock.__init__(self.epoch_length)
        threading.Thread.__init__(self)
        self.daemon=True
        self.epoch_length=epoch_length # how many slots in a a block
        self.node = node #stakeholder
        self.vrf = VRF(self.node.vrf_pk, self.node.vrf_sk, self.node.vrk_base)
        self.current_slot = self.slot

    def run(self):
        self.__background()

    def __background(self):
        current_epoch = self.slot
        while True:
            if self.slot != current_epoch:
                current_epoch = self.slot
                self.__callback()

    def __callback(self):
        self.current_slot = self.slot
        sigma, proof = self.vrf.sign(self.current_slot)
        if self.slot%self.epoch_length==0:
            self.node.new_slot(self.current_slot, sigma, proof)
        else:
            self.node.new_slot(self.current_slot, sigma, proof, True)

    def verify(self, y, pi, pk_raw, g):
        return VRF.verify(self.current_slot, y, pi, pk_raw, g)
    