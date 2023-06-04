#!/usr/bin/env python3

from argparse import ArgumentParser
from darkfi_sdk_py.affine import Affine
from darkfi_sdk_py.base import Base
from darkfi_sdk_py.scalar import Scalar
from darkfi_sdk_py.proof import Proof
from darkfi_sdk_py.proving_key import ProvingKey
from darkfi_sdk_py.point import Point
from darkfi_sdk_py.verifying_key import VerifyingKey
from darkfi_sdk_py.zk_circuit import ZkCircuit
from darkfi_sdk_py.zk_binary import ZkBinary
from time import time
from sys import getsizeof

def insert_heap(heap, element):
    print(f"Heap before: {heap}, element: {element}")
    heap.append(element)

def insert_publics(publics, element):
    print(f"Publics before: {publics}, element: {element}")
    publics.append(element)
   
def get_publics(statements, witnesses, constant_count, literals):
    # Python heap for executing zk statements
    heap = [None] * constant_count + witnesses
    publics = []
    for stmt in statements:
        print('---------------- BEGIN ------------------')
        print(f"Statement: {stmt}")
        opcode, args = stmt[0], stmt[1]
        if opcode == 'BaseAdd':
            a = heap[args[0][1]]
            b = heap[args[1][1]]
            insert_heap(heap, a.add(b))
        elif opcode == 'BaseMul':
            a = heap[args[0][1]]
            b = heap[args[1][1]]
            insert_heap(heap, a.mul(b))
        elif opcode == 'BaseSub':
            a = heap[args[0][1]]
            b = heap[args[1][1]]
            insert_heap(heap, a.sub(b))
        elif opcode == 'EcAdd':
            a = heap[args[0][1]]
            b = heap[args[1][1]]
            insert_heap(heap, a.add(b))
        elif opcode == 'EcMul':
            a = heap[args[0][1]]
            insert_heap(heap, Point.mul_r_generator(a))
        elif opcode in {'EcMulBase', 'EcMulVarBase'}:
            i = args[0][1]
            base = heap[i]
            product = Point.mul_base(base)
            insert_heap(heap, product)
        elif opcode == 'EcMulShort':
            value = heap[args[0][1]]
            insert_heap(heap, Point.mul_short(value))
        elif opcode == 'EcGetX':
            i = args[0][1]
            point = heap[i] 
            x, _ = point.to_affine().coordinates()
            insert_heap(heap, x)
        elif opcode == 'EcGetY':
            i = args[0][1]
            point = heap[i] 
            _, y = point.to_affine().coordinates()
            insert_heap(heap, y)
        elif opcode == 'PoseidonHash':
            messages = [heap[m[1]] for m in args]
            insert_heap(heap, Base.poseidon_hash(messages))
        elif opcode == 'MerkleRoot':
            i = heap[args[0][1]]
            p = heap[args[1][1]]
            a = heap[args[2][1]]
            insert_heap(heap, Base.merkle_root(i, p, a))
        elif opcode == 'ConstrainInstance':
            i = args[0][1]
            element = heap[i] 
            insert_publics(publics, element)
        elif opcode == 'WitnessBase':
            type = args[0][0]
            assert type == 'Lit', f"type should LitType instead of {type}"
            print(args)
            i = args[0][1]
            element = int(literals[i][1]) # (LitType, Lit)
            base = Base(element)
            insert_heap(heap, base)        
        elif opcode == 'CondSelect':
            cnd = heap[args[0][1]]
            thn = heap[args[1][1]]
            els = heap[args[2][1]]
            assert cnd.eq(Base(0)) or cnd.eq(Base(1)), "Failed bool check"
            res = thn if cnd.eq(Base(1)) else els
            insert_heap(heap, res)
        elif opcode in IGNORED_OPCODES:
            print(f"Processed opcode: {opcode}")
        else:
            print(f"Missing implementation: {opcode}")
    
    print("-------------------- END --------------------")

    print("-----------------------------------")
    print(f"Publics: {publics}")
    print("-----------------------------------")
    
    return publics

def bincode_data(bincode):
    with open(bincode, "rb") as f:
        bincode = f.read()
        zkbin = ZkBinary.decode(bincode)
        return {"zkbin": zkbin,
                "namespace": zkbin.namespace(),
                "witnesses": zkbin.witnesses(),
                "constant_count": zkbin.constant_count(),
                "statements": zkbin.opcodes(),
                "literals": zkbin.literals()}
    
IGNORED_OPCODES = {
    'Noop',
    'RangeCheck',
    'LessThanStrict',
    'LessThanLoose',
    'BoolCheck',
    'ConstrainEqualBase',
    'ConstrainEqualPoint',
    'DebugPrint'
}
K = 13

if __name__ ==  "__main__":

    ##### Script inputs #####

    bincode_path = "opcodes.no-nipoint.zk.bin"
    # bincode_path = "../../example/simple.zk.bin"
    bincode_data_ = bincode_data(bincode_path)
    zkbin, statements, constant_count, literals = bincode_data_['zkbin'], bincode_data_['statements'], bincode_data_['constant_count'], bincode_data_['literals']
    witnesses = [
        Base(3),
        Scalar(4),
        Base(5),
        Base(6),
        Base(7),
        Base(8),
        10,
        [Base(42)] * 32,
        Base(1),
    ]
    
    ##### Proving #####
    
    print("Making public inputs based off witnesses......")
    publics = get_publics(statements, witnesses, constant_count, literals)
    
    print("Witnessing into prover's circuit.....")
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
    proof = Proof.create(proving_key, [zkcircuit], publics)
    print(f"Time for proving: {time() - start}")
    
    
    ##### Verifiying #####
    
    zkcircuit_v = zkcircuit.verifier_build(zkbin)
    
    print(f"Making verifying key.....")
    start = time()
    verifying_key = VerifyingKey.build(K, zkcircuit_v)
    print(f"Time for making verifying key: {time() - start}")
    
    print("Verifying.....")
    start = time()
    proof.verify(verifying_key, publics)
    print(f"Time for verifying {time() - start}")
    