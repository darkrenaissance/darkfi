from ouroboros import Stakeholder
from ouroboros import Z
import time

EPOCH_LENGTH = 3
stakeholders = []
for i in range(3):
    stakeholders.append(Stakeholder(EPOCH_LENGTH))

environment = Z(stakeholders, EPOCH_LENGTH, genesis_time=time.time())

environment.start()

for sh in environment.stakeholders:
    sh.beacon.join()
