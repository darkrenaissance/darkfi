#!/usr/bin/env python3
# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2026 Dyne.org foundation
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, either version 3 of the
# License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Example witness generation for some circuit.
Here we generate them for opcodes.zk

This reflects /darkfi/tests/zkvm_opcodes.rs
"""
import json
from darkfi_sdk.pasta import Ep, Fp, Fq, nullifier_k, EpAffine, fp_mod_fv
from darkfi_sdk.crypto import poseidon_hash, pedersen_commitment_u64
from darkfi_sdk.merkle import MerkleTree

# Creating base elements and scalars
value = 666
value_blind = Fq.random()
blind = Fp.random()
secret = Fp.random()
a = Fp.from_u64(42)
b = Fp.from_u64(69)

# Creating a Merkle tree (internally using bridgetree)
tree = MerkleTree()
c0 = Fp.random()
c1 = Fp.random()
c2 = poseidon_hash([Fp.one(), Fp.from_u64(2), blind])
c3 = Fp.random()

# Appending and marking leaves in the Merkle tree
tree.append(c0)
tree.mark()
tree.append(c1)
tree.append(c2)
leaf_pos = tree.mark()
tree.append(c3)
tree.mark()

# Calculating the tree root and authentication path
root = tree.root(0)
path = tree.witness(leaf_pos, 0)

# Elliptic curve multiplication
ephem_secret = Fp.random()
pubkey = Ep.from_affine(nullifier_k()) * fp_mod_fv(ephem_secret)

ephem_public = pubkey * fp_mod_fv(ephem_secret)
ephem_x, ephem_y = EpAffine.from_projective(ephem_public).coordinates()

value_commit = pedersen_commitment_u64(value, value_blind)
value_coords = EpAffine.from_projective(value_commit).coordinates()
d = poseidon_hash([Fp.one(), blind, value_coords[0], value_coords[1]])

public = Ep.from_affine(nullifier_k()) * fp_mod_fv(secret)
pub_x, pub_y = EpAffine.from_projective(public).coordinates()

# Create the object representing the JSON witnesses file.
w = {}

# Private witnesses for the proof
# yapf: disable
w["witnesses"] = [
    {"Base": str(Fp.from_u64(value))},
    {"Scalar": str(value_blind)},
    {"Base": str(blind)},
    {"Base": str(a)},
    {"Base": str(b)},
    {"Base": str(secret)},
    {"EcNiPoint": EpAffine.from_projective(pubkey).coordinates_str()},
    {"Base": str(ephem_secret)},
    {"Uint32": leaf_pos},
    {"MerklePath": [str(i) for i in path]},
    {"Base": str(Fp.one())},
]

# Public inputs for the proof
# yapf: disable
w["instances"] = [
    str(value_coords[0]),
    str(value_coords[1]),
    str(c2),
    str(d),
    str(root),
    str(pub_x),
    str(pub_y),
    str(ephem_x),
    str(ephem_y),
    str(a),
    str(Fp.zero()),
]

# Printing the expected JSON file used by zkrunner.
print(json.dumps(w, indent=2))
