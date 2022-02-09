from dpos.ouroboros import Stakeholder
from dpos.ouroboros import Z
import random

EPOCH_LENGTH = 7
stakeholders = []
for i in range(3):
    stakeholders.append(Stakeholder)(EPOCH_LENGTH)

environment = Z(stakeholders, EPOCH_LENGTH)

environment.start()
