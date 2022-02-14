import threading
from ouroboros.clock import SynchedNTPClock
from ouroboros.vrf import VRF
from ouroboros.logger import Logger

'''
\class TrustedBeacon

the trusted beacon is decentralized, such that at the onset of the Epoch,
the leader of the first slot generated the signed seed, and release the signature, 
proof, and base to the genesis block.

#TODO implement trustedbeacon as a node
'''
class TrustedBeacon(SynchedNTPClock, threading.Thread):
    def __init__(self, node, vrf, epoch_length, genesis_time):
        self.epoch_length=epoch_length # how many slots in a a block
        SynchedNTPClock.__init__(self)
        threading.Thread.__init__(self)
        self.daemon=True
        self.node = node #stakeholder
        self.vrf = vrf
        self.current_slot = self.slot
        self.log = Logger(self, genesis_time)
        self.log.info(f"constructed for node {str(node)}")
        self.bb=0 # epoch counts since genesis (big bang)

    def __repr__(self):
        return f"trustedbeacon"

    def run(self):
        self.log.highlight("thread [start]")
        self.__background()
        self.log.info("thread [end]")

    def __background(self):
        current_epoch = self.slot
        self.log.info('background waiting for the onset of next synched epoch...')
        while True:
            if self.slot != current_epoch:
                current_epoch = self.slot
                self.__callback()

    def __callback(self):
        self.current_slot = self.slot
        if self.current_slot%self.epoch_length!=0:
            if self.bb==0:
                # new nodes attached to the network, need to either request old blocks, or wait for next epoch's broadcst
                # it's temporarily, and or simplicity set to the latter 
                return
            self.log.info(f"callback: new slot of idx: {self.current_slot}")
            #y, pi = self.vrf.sign(self.current_slot)
            self.log.info(f"callbaxck: signature calculated for {str(self.node)}")
            self.node.new_slot(self.current_slot)
        else:
            self.bb+=1
            sigmas = []
            proofs = []
            #TODO since it's expensive, but to generate single (y,pi) pair as seed 
            # and use random hash function to generate the rest randomly. 
            if self.node.am_current_leader:
                for i in range(self.epoch_length):
                    self.log.info(f"callback: new slot of idx: {self.current_slot}, epoch slot {i}")
                    y, pi = self.vrf.sign(self.current_slot)
                    self.log.info(f"callback: signature calculated for {str(self.node)}")
                    sigmas.append(y)
                    proofs.append(pi)
            self.node.new_epoch(self.current_slot, sigmas, proofs)

    def verify(self, y, pi, pk_raw, g):
        return VRF.verify(self.current_slot, y, pi, pk_raw, g)
