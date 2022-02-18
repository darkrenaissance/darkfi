import time
from ouroboros import Stakeholder
from ouroboros import Z


EPOCH_LENGTH = 3
stakeholders = []

for i in range(3):
    stakeholders.append(Stakeholder(EPOCH_LENGTH, 'passwd'+str(i)))

environment = Z(stakeholders, EPOCH_LENGTH, genesis_time=time.time())
