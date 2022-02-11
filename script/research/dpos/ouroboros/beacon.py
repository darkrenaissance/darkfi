from ouroboros.clock import SynchedNTPClock
from ouroboros.vrf import VRF
from ouroboros.logger import Logger
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
    def __init__(self, node, vrf_sk, epoch_length, genesis_time):
        self.epoch_length=epoch_length # how many slots in a a block
        SynchedNTPClock.__init__(self)
        threading.Thread.__init__(self)
        self.daemon=True
        self.node = node #stakeholder
        self.vrf = VRF(self.node.vrf_pk, vrf_sk, self.node.vrf_base)
        self.current_slot = self.slot
        self.log = Logger(self, genesis_time)
        self.log.info(f"[TrustedBeacon] constructed for node {str(node)}")

    def __repr__(self):
        return f"trustedbeacon"

    def run(self):
        self.log.info("[TrustedBeacon] thread [start]")
        self.__background()
        self.log.info("[TrustedBeacon] thread [end]")

    def __background(self):
        current_epoch = self.slot
        while True:
            if self.slot != current_epoch:
                current_epoch = self.slot
                self.__callback()

    def __callback(self):
        self.current_slot = self.slot
        sigmas = []
        proofs = []
        for i in range(self.epoch_length):
            self.log.info(f"[TrustedBeacon] callback: new slot of idx: {self.current_slot}, epoch slot {i}")
            y, pi = self.vrf.sign(self.current_slot)
            self.log.info(f"[TrustedBeacon] callback: signature calculated for {str(self.node)}")
            sigmas.append(y)
            proofs.append(pi)
        if self.current_slot%self.epoch_length==0:
            self.log.info(["[TrustedBeacon] new slot"])
            self.node.new_slot(self.current_slot, sigmas[0], proofs[0])
        else:
            self.log.info([f"[TrustedBeacon] new epoch with simgas of size:{len(sigmas)}, proofs: {len(proofs)}"])
            self.node.new_epoch(self.current_slot, sigmas, proofs)

    def verify(self, y, pi, pk_raw, g):
        return VRF.verify(self.current_slot, y, pi, pk_raw, g)
