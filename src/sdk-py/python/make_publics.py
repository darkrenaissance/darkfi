#!/usr/bin/env python3

from darkfi_sdk_py import Base
from darkfi_sdk_py import Scalar
from darkfi_sdk_py import Point
from darkfi_sdk_py import Affine
# ZkBinary        -> wrap zk.bin
# Witness         -> wrap witness(es)
# ZkCircuit       <- (Witnesses, ZkBinary)
# Proving Key     <- ProvingKey::build(k, Circuit)
# Proof           <- Proof::create(ProvingKey, Circuit, publics, rng)
# Verifiying Key  <- proof.verify(verify_key, publics)