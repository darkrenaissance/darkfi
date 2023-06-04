#!/usr/bin/env python3

"""
Script for playing around with the Python SDK
"""

from darkfi_sdk_py import Base
from darkfi_sdk_py import Scalar
from darkfi_sdk_py import Point
from darkfi_sdk_py import Affine
from darkfi_sdk_py import Proof
from darkfi_sdk_py import VerifyingKey
from darkfi_sdk_py import ProvingKey
from darkfi_sdk_py import Affine
from darkfi_sdk_py import ZkCircuit
from darkfi_sdk_py import ZkBinary
from time import time
from sys import getsizeof

##### get circuit #####

f = open("simple.zk.bin", "rb")
bincode = f.read()
f.close()
print(f"bincode {bincode}")
zkbin = ZkBinary.decode(bincode)
print(f"zkbin {zkbin}")

##### prover #####
k = 13
value = 42
value_blind = Scalar.random()

zkcircuit = ZkCircuit(zkbin)
zkcircuit.witness_base(Base.from_u128(value))
zkcircuit.witness_scalar(value_blind)
zkcircuit = zkcircuit.build(zkbin)

##### proving key #####
print("making proving key...")
proving_key = ProvingKey.build(k, zkcircuit)

# pedersen commitment
comm = Point.mul_short(value)
comm_r = Point.blinding_point(value_blind)
valcom = comm.add(comm_r)
print(f"valcom {valcom}")
(x, y) = valcom.to_affine().coordinates()
print(f"x {x}")
print(f"y {y}")
print(x)
print(y)
publics = [x, y]

start = time()
print("making proof...")
proof = Proof.create(proving_key, [zkcircuit], publics)
print(f"time {time() - start}")


#################### VERIFICATION

print("verification starts.....")

start = time()
zkcircuit_v = zkcircuit.verifier_build(zkbin)

print(f"building verifying key")
start = time()
# IMPORTANT QUESTION: can this be uploaded to an eth smart contract
verifying_key = VerifyingKey.build(k, zkcircuit_v)
print(f"time {time() - start}")


print(f"verifying")
start = time()
proof.verify(verifying_key, publics)
print(f"time {time() - start}")

print(f"size of proof        : {getsizeof(proof)}")
print(f"size of proving key  : {getsizeof(proving_key)}")
print(f"size of verifying key: {getsizeof(verifying_key)}")

## SHOULD FAILLLLLL
proof.verify(verifying_key, [x])