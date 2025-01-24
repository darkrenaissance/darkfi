#!/usr/bin/env python3
# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2025 Dyne.org foundation
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
Python tool to prototype zkVM proofs given zkas source code and necessary
witness values in JSON format.
"""
import json
import sys
from darkfi_sdk.pasta import Fp, Fq, Ep
from darkfi_sdk.zkas import (MockProver, ZkBinary, ZkCircuit, ProvingKey,
                             Proof, VerifyingKey)

def eprint(fstr, *args):
    print("error: " + fstr, *args, file=sys.stderr)

def show_trace(opcodes, trace):
    print(f"{'Line':<4} {'Opcode':<22} {'Type':<10} {'Values'}")
    for i, (opcode, (optype, args)) in enumerate(zip(opcodes, trace)):
        if args:
            args = ", ".join([str(arg) for arg in args])
            args = f"[{args}]"
        else:
            args = ""
        opcode = str(opcode)
        optype = str(optype)
        print(f"{i:<4} {opcode:<22} {optype:<10} {args}")

def load_circuit_witness(circuit, witness_file):
    # We attempt to decode the witnesses from the JSON file.
    # Refer to the `witness_gen.py` file to see what the format of this
    # file should be.
    if witness_file == "-":
        witness_data = json.load(sys.stdin)
    else:
        with open(witness_file, "r", encoding="utf-8") as json_file:
            witness_data = json.load(json_file)

    # Now we scan through the parsed JSON witness file and
    # build our "heap". These will be appended to the initial
    # circuit and decide the code path for the prover.
    for witness in witness_data["witnesses"]:
        assert len(witness) == 1
        if (value := witness.get("EcPoint")) is not None:
            circuit.witness_ecpoint(Ep(value))

        elif (value := witness.get("EcNiPoint")) is not None:
            assert len(value) == 2
            xcoord, ycoord = Fp(value[0]), Fp(value[1])
            circuit.witness_ecnipoint(Ep(xcoord, ycoord))

        elif (value := witness.get("Base")) is not None:
            circuit.witness_base(Fp(value))

        elif (value := witness.get("Scalar")) is not None:
            circuit.witness_scalar(Fq(value))

        elif (value := witness.get("MerklePath")) is not None:
            path = [Fp(i) for i in value]
            assert len(path) == 32
            circuit.witness_merklepath(path)

        elif (value := witness.get("SparseMerklePath")) is not None:
            path = [Fp(i) for i in value]
            assert len(path) == 255
            circuit.witness_sparsemerklepath(path)

        elif (value := witness.get("Uint32")) is not None:
            print("here")
            circuit.witness_uint32(value)

        elif (value := witness.get("Uint64")) is not None:
            circuit.witness_uint64(value)

        else:
            eprint(f"Invalid Witness type for witness {witness}")
            return -1

    # Instances are our public inputs for the proof and they're also
    # part of the JSON file.
    instances = []
    for instance in witness_data["instances"]:
        instances.append(Fp(instance))
    return instances

def main(witness_file, source_file, mock=False, trace=False):
    """main zkrunner logic"""
    # Then we attempt to compile the given zkas code and create a
    # zkVM circuit. This compiling logic happens in the Python bindings'
    # `ZkBinary::new` function, and should be equivalent to the actual
    # `zkas` binary provided in the DarkFi codebase.
    print("Compiling zkas code...")
    with open(source_file, "r", encoding="utf-8") as zkas_file:
        zkas_source = zkas_file.read()

    # This line will compile the source code
    zkbin = ZkBinary(source_file, zkas_source)

    # Construct the initial circuit object.
    circuit = ZkCircuit(zkbin)
    print("Decoding witnesses...")
    instances = load_circuit_witness(circuit, witness_file)

    # If we want to build an actual proof, we'll need a proving key
    # and a verifying key.
    # circuit.verifier_build() is called so that the inital circuit
    # (which contains no witnesses) actually calls empty_witnesses()
    # in order to have the correct code path when the circuit gets
    # synthesized.
    if not mock:
        print("Building proving key...")
        proving_key = ProvingKey.build(zkbin.k(), circuit.verifier_build())

        print("Building verifying key...")
        verifying_key = VerifyingKey.build(zkbin.k(), circuit.verifier_build())

    # circuit.prover_build() will actually construct the circuit
    # with the values witnessed above.
    circuit = circuit.prover_build()
    if trace:
        if mock:
            eprint(f"Debug trace can only be enabled with --prove")
            return -2
        circuit.enable_trace()

    # If we're building an actual proof, we'll use the ProvingKey to
    # prove and our VerifyingKey to verify the proof.
    if not mock:
        print("Proving knowledge of witnesses...")
        proof = Proof.create(proving_key, [circuit], instances)
        if proof is None:
            eprint(f"Proof creation failed")
            return -3

        if trace:
            show_trace(zkbin.opcodes(), circuit.opvalues())

        print("Verifying ZK proof...")
        verify_status = proof.verify(verifying_key, instances)

    # Otherwise, we'll simply run the MockProver:
    else:
        print("Running MockProver...")
        proof = MockProver.run(zkbin.k(), circuit, instances)

        print("Verifying MockProver...")
        verify_status = proof.verify()

    if not verify_status:
        eprint("Proof failed to verify")
        return -3

    print("Proof verified successfully!")
    return 0

if __name__ == "__main__":
    from argparse import ArgumentParser

    parser = ArgumentParser(
        prog="zkrunner",
        description="Python util for running zk proofs",
        epilog="This tool is only for prototyping purposes",
    )

    parser.add_argument(
        "SOURCE",
        help="Path to zkas source code",
    )
    parser.add_argument(
        "-w",
        "--witness",
        required=True,
        help="Path to JSON file holding witnesses",
    )
    parser.add_argument(
        "--prove",
        action="store_true",
        help="Actually create a real proof instead of using MockProver",
    )
    parser.add_argument(
        "--trace",
        action="store_true",
        help="Enable debug trace (only works with --prove enabled)",
    )

    args = parser.parse_args()
    sys.exit(main(args.witness, args.SOURCE, mock=not args.prove,
                  trace=args.trace))

