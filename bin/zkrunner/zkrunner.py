#!/usr/bin/env python3

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


def heap_add(heap, element):
    print(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> heap")
    pprint(heap)
    print(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> to add")
    pprint(element)
    heap.append(element)


def pubins_add(pubins, element):
    print(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> pubins")
    pprint(pubins)
    print(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> to add")
    pprint(element)
    pubins.append(element)


def get_pubins(statements, witnesses, constant_count, literals):
    # Python heap for executing zk statements
    heap = [None] * constant_count + witnesses
    pubins = []
    for stmt in statements:
        print(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> Statement")
        print(stmt)
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
            print(">>>>>>>>>>>>>>>>>>>>>>>>>>>>> Ignored opcode")
            pprint(opcode)
        else:
            print(
                ">>>>>>>>>>>>>>>>>>>>>>>>>>>>> Missing implementation for opcode"
            )
            pprint(opcode)
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
            "literals": zkbin.literals()
        }


IGNORED_OPCODES = {
    'Noop', 'RangeCheck', 'LessThanStrict', 'LessThanLoose', 'BoolCheck',
    'ConstrainEqualBase', 'ConstrainEqualPoint', 'DebugPrint'
}
K = 13

if __name__ == "__main__":

    ##### Script inputs #####

    bincode_path = "opcodes.no-nipoint.zk.bin"
    # bincode_path = "../../example/simple.zk.bin" # You must change the witnesses as well
    bincode_data_ = bincode_data(bincode_path)
    zkbin, statements, constant_count, literals = bincode_data_[
        'zkbin'], bincode_data_['statements'], bincode_data_[
            'constant_count'], bincode_data_['literals']
    witnesses = [
        Base.from_u64(3),
        Scalar.from_u64(4),
        Base.from_u64(5),
        Base.from_u64(6),
        Base.from_u64(7),
        Base.from_u64(8),
        10,
        [Base.from_u64(42)] * 32,
        Base.from_u64(1),
    ]

    ##### Proving #####

    print("Making public inputs based off of witnesses......")
    pubins = get_pubins(statements, witnesses, constant_count, literals)

    print("Witnessing each witness into prover's circuit.....")
    zkcircuit = ZkCircuit(zkbin)
    zkcircuit.witness_base(witnesses[0])
    zkcircuit.witness_scalar(witnesses[1])
    zkcircuit.witness_base(witnesses[2])
    zkcircuit.witness_base(witnesses[3])
    zkcircuit.witness_base(witnesses[4])
    zkcircuit.witness_base(witnesses[5])
    zkcircuit.witness_u32(witnesses[6])
    zkcircuit.witness_merkle_path(witnesses[7])
    zkcircuit.witness_base(witnesses[8])
    zkcircuit = zkcircuit.build(zkbin)

    print("Making proving key.....")
    start = time()
    proving_key = ProvingKey.build(K, zkcircuit)
    print(f"Time for making proving key: {time() - start}")

    print("Proving.....")
    start = time()
    proof = Proof.create(proving_key, [zkcircuit], pubins)
    print(f"Time for proving: {time() - start}")

    ##### Verifiying #####

    zkcircuit_v = zkcircuit.verifier_build(zkbin)

    print(f"Making verifying key.....")
    start = time()
    verifying_key = VerifyingKey.build(K, zkcircuit_v)
    print(f"Time for making verifying key: {time() - start}")

    print("Verifying.....")
    start = time()
    proof.verify(verifying_key, pubins)
    print(f"Time for verifying {time() - start}")
