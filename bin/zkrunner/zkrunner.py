from argparse import ArgumentParser
from darkfi_sdk_py.affine import Affine
from darkfi_sdk_py.base import Base
from darkfi_sdk_py.point import Point
from darkfi_sdk_py.proof import Proof
from darkfi_sdk_py.proving_key import ProvingKey
from darkfi_sdk_py.scalar import Scalar
from darkfi_sdk_py.verifying_key import VerifyingKey
from darkfi_sdk_py.zk_binary import ZkBinary
from darkfi_sdk_py.zk_circuit import ZkCircuit
from pprint import pprint
from sys import getsizeof
from time import time
import argparse


def heap_add(heap, element):
    heap.append(element)
    vprint(f"HEAP: {heap}")


def pubins_add(pubins, element):
    pubins.append(element)
    vprint(f"PUBLIC INPUTS: {pubins}")


def get_pubins(statements, witnesses, constant_count, literals):
    # Python heap for executing zk statements
    heap = [None] * constant_count + witnesses
    pubins = []
    for stmt in statements:
        vprint(f"STATEMENT: {stmt}")
        opcode, args = stmt[0], stmt[1]
        if opcode == 'BaseAdd':
            a = heap[args[0][1]]
            b = heap[args[1][1]]
            heap_add(heap, a + b)
        elif opcode == 'BaseMul':
            a = heap[args[0][1]]
            b = heap[args[1][1]]
            heap_add(heap, a * b)
        elif opcode == 'BaseSub':
            a = heap[args[0][1]]
            b = heap[args[1][1]]
            heap_add(heap, a - b)
        elif opcode == 'EcAdd':
            a = heap[args[0][1]]
            b = heap[args[1][1]]
            heap_add(heap, a + b)
        elif opcode == 'EcMul':
            a = heap[args[0][1]]
            heap_add(heap, Point.mul_r_generator(a))
        elif opcode in {'EcMulBase', 'EcMulVarBase'}:
            i = args[0][1]
            base = heap[i]
            product = Point.mul_base(base)
            heap_add(heap, product)
        elif opcode == 'EcMulShort':
            value = heap[args[0][1]]
            heap_add(heap, Point.mul_short(value))
        elif opcode == 'EcGetX':
            i = args[0][1]
            point = heap[i]
            x, _ = point.to_affine().coordinates()
            heap_add(heap, x)
        elif opcode == 'EcGetY':
            i = args[0][1]
            point = heap[i]
            _, y = point.to_affine().coordinates()
            heap_add(heap, y)
        elif opcode == 'PoseidonHash':
            messages = [heap[m[1]] for m in args]
            heap_add(heap, Base.poseidon_hash(messages))
        elif opcode == 'MerkleRoot':
            i = heap[args[0][1]]
            p = heap[args[1][1]]
            a = heap[args[2][1]]
            heap_add(heap, Base.merkle_root(i, p, a))
        elif opcode == 'ConstrainInstance':
            i = args[0][1]
            element = heap[i]
            pubins_add(pubins, element)
        elif opcode == 'WitnessBase':
            type = args[0][0]
            assert type == 'Lit', f"type should LitType instead of {type}"
            i = args[0][1]
            element = int(literals[i][1])  # (LitType, Lit)
            base = Base.from_u64(element)
            heap_add(heap, base)
        elif opcode == 'CondSelect':
            cnd = heap[args[0][1]]
            thn = heap[args[1][1]]
            els = heap[args[2][1]]
            assert cnd == Base.from_u64(0) or cnd == Base.from_u64(
                1), "Failed bool check"
            res = thn if cnd == Base.from_u64(1) else els
            heap_add(heap, res)
        elif opcode in IGNORED_OPCODES:
            vprint(f"IGNORE: {opcode}")
        else:
            vprint(f"NO IMPLEMENTATION: {opcode}")
    return pubins


def bincode_data(bincode):
    with open(bincode, "rb") as f:
        bincode = f.read()
        zkbin = ZkBinary.decode(bincode)
        return {
            "zkbin": zkbin,
            "namespace": zkbin.namespace(),
            "witnesses": zkbin.witnesses(),
            "constant_count": zkbin.constant_count(),
            "statements": zkbin.opcodes(),
            "literals": zkbin.literals(),
            "k": zkbin.k()
        }


IGNORED_OPCODES = {
    'Noop', 'RangeCheck', 'LessThanStrict', 'LessThanLoose', 'BoolCheck',
    'ConstrainEqualBase', 'ConstrainEqualPoint', 'DebugPrint'
}

if __name__ == "__main__":

    # TODO: relative path to your zkas binary
    bincode_path = "set_v1.zk.bin"

    ##### Setup #####

    bincode_data_ = bincode_data(bincode_path)
    zkbin = bincode_data_['zkbin']
    statements = bincode_data_['statements']
    constant_count = bincode_data_['constant_count']
    literals = bincode_data_['literals']
    K = bincode_data_['k']

    ##### TODO: Your Inputs #####

    # TODO: list of witnesses, in the same order as in the zkas circuit witness section
    witnesses = [
        Base.from_u64(42),
        Base.from_u64(1),
        Base.from_u64(1),
        Base.from_u64(1),
        Base.from_u64(1),
    ]

    zkcircuit = ZkCircuit(zkbin)

    # TODO: call the corresponding witness_* prefixed methods to assign the witness
    # to the circuit. For a complete list, `rgrep witness_ <darkfi>/src/sdk/python`
    zkcircuit.witness_base(witnesses[0])
    zkcircuit.witness_base(witnesses[1])
    zkcircuit.witness_base(witnesses[2])
    zkcircuit.witness_base(witnesses[3])
    zkcircuit.witness_base(witnesses[4])

    zkcircuit = zkcircuit.build(zkbin)

    # Verbosity
    parser = argparse.ArgumentParser()
    verbose = parser.add_argument(
        '--verbose', action='store_true', help='verbose switch')
    args = parser.parse_args()
    vprint = print if args.verbose else lambda *a, **k: None

    ##### Proving #####

    pubins = get_pubins(statements, witnesses, constant_count, literals)

    vprint("Making proving key.....")
    start = time()
    proving_key = ProvingKey.build(K, zkcircuit)
    print(f"Time for making proving key: {time() - start}")

    vprint("Proving.....")
    start = time()
    proof = Proof.create(proving_key, [zkcircuit], pubins)
    # TODO: consider persisting the proof for making a transaction
    print(f"Time for proving: {time() - start}")

    ##### Verifiying #####

    zkcircuit_v = zkcircuit.verifier_build(zkbin)

    vprint(f"Making verifying key.....")
    start = time()
    verifying_key = VerifyingKey.build(K, zkcircuit_v)
    print(f"Time for making verifying key: {time() - start}")

    vprint("Verifying.....")
    start = time()
    vprint(f"PUBLIC INPUTS: {pubins}")
    proof.verify(verifying_key, pubins)
    print(f"Time for verifying {time() - start}")
