from ouroboros import Stakeholder
from ouroboros import Z
import time

EPOCH_LENGTH = 2
stakeholders = []

for i in range(2):
    stakeholders.append(Stakeholder(EPOCH_LENGTH))

environment = Z(stakeholders, EPOCH_LENGTH, genesis_time=time.time())
environment.start()

for sh in environment.stakeholders:
    sh.beacon.join()
