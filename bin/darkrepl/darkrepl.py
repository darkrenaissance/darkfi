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

###############################
# tpyes should be defined in the types provided by bindings
# the bindings should yield to python usages, not the other way

# all comparing opcodes are not evaluated
# only opcodes that return a value are evaluated, for making publics
# you should expect to see errors upon verification

################################
### Art
################################

################################
### Parsed Bincode
################################
f = open("opcodes.no-nipoint.zk.bin", "rb")
bincode = f.read()
f.close()
zkbin = ZkBinary.decode(bincode)

namespace = zkbin.namespace()
print(f"namespace: {namespace}")

witnesses = zkbin.witnesses()
print(f"witnesses: {witnesses}")

constant_count = zkbin.constant_count()
print(f"constant count: {constant_count}")

stmts = zkbin.opcodes()
print(f"opcodes: {stmts}")

literals = zkbin.literals()
print(f"literals: {literals}")

################################
### Accept Witnesses
################################

# test witnesses
witnesses = [
    Base.from_u64(3),
    Scalar.from_u128(4),
    Base.from_u64(5),
    Base.from_u64(6),
    Base.from_u64(7),
    Base.from_u64(8),
    # Point.generator().mul_base(Base.from_u64(42)), # NiPoint
    # Base.from_u64(9),
    10,
    [Base.from_u64(42)] * 32,
    Base.from_u64(1),
]

################################
### Heap and Literals
################################

# IMPORTANT: how should the values be encoded (scalar/object) on the Python heap?
heap = [None] * constant_count + witnesses
publics = []

####### User Facing ##########

################################
### Publics
################################

# Opcode reference
# 
# Intermediate opcode for the compiler, should never appear in the result
# Noop = 0x00,
# 
# Elliptic curve addition
# EcAdd = 0x01,
# 
# Elliptic curve multiplication
# EcMul = 0x02,
# 
# Elliptic curve multiplication with a Base field element
# EcMulBase = 0x03,
# 
# Elliptic curve multiplication with a Base field element of 64bit width
# EcMulShort = 0x04,
# 
# Variable Elliptic curve multiplication with a Base field element
# EcMulVarBase = 0x05,
# 
# Get the x coordinate of an elliptic curve point
# EcGetX = 0x08,
# 
# Get the y coordinate of an elliptic curve point
# EcGetY = 0x09,
# 
# Poseidon hash of N Base field elements
# PoseidonHash = 0x10,
# 
# Calculate Merkle root, given a position, Merkle path, and an element
# MerkleRoot = 0x20,
# 
# Base field element addition
# BaseAdd = 0x30,
# 
# Base field element multiplication
# BaseMul = 0x31,
# 
# Base field element subtraction
# BaseSub = 0x32,
# 
# Witness an unsigned integer into a Base field element
# WitnessBase = 0x40,
# 
# Range check a Base field element, given bit-width (up to 253)
# RangeCheck = 0x50,
# 
# Strictly compare two Base field elements and see if a is less than b
# This enforces the sum of remaining bits to be zero.
# LessThanStrict = 0x51,
# 
# Loosely two Base field elements and see if a is less than b
# This does not enforce the sum of remaining bits to be zero.
# LessThanLoose = 0x52,
# 
# Check if a field element fits in a boolean (Either 0 or 1)
# BoolCheck = 0x53,
# 
# Conditionally select between two base field elements given a boolean
# CondSelect = 0x60,
# 
# Constrain equality of two Base field elements inside the circuit
# ConstrainEqualBase = 0xe0,
# 
# Constrain equality of two EcPoint elements inside the circuit
# ConstrainEqualPoint = 0xe1,
# 
# Constrain a Base field element to a circuit's public input
# ConstrainInstance = 0xf0,
# 
# Debug a variable's value in the ZK circuit table.
# DebugPrint = 0xff,


def insert_heap(element):
    print(f"heap before: {heap}, element: {element}")
    heap.append(element)

def insert_publics(element):
    print(f"publics before: {publics}, element: {element}")
    publics.append(element)

ignored = {
    'Noop',
    'RangeCheck',
    'LessThanStrict',
    'LessThanLoose',
    'BoolCheck',
    'ConstrainEqualBase',
    'ConstrainEqualPoint',
    'DebugPrint'
}
for stmt in stmts:
    print('---------------- BEGIN ------------------')
    print(stmt)
    opcode, args = stmt[0], stmt[1]
    
    if opcode == 'BaseAdd':
        a = heap[args[0][1]]
        b = heap[args[1][1]]
        insert_heap(a.add(b))
    elif opcode == 'BaseMul':
        a = heap[args[0][1]]
        b = heap[args[1][1]]
        insert_heap(a.mul(b))
    elif opcode == 'BaseSub':
        a = heap[args[0][1]]
        b = heap[args[1][1]]
        insert_heap(a.sub(b))
    elif opcode == 'EcAdd':
        a = heap[args[0][1]]
        b = heap[args[1][1]]
        insert_heap(a.add(b))
    elif opcode == 'EcMul':
        a = heap[args[0][1]]
        insert_heap(Point.blinding_point(a))
    elif opcode in {'EcMulBase', 'EcMulVarBase'}:
        i = args[0][1]
        base = heap[i]
        product = Point.mul_base(base)
        insert_heap(product)
    elif opcode == 'EcMulShort':
        value = heap[args[0][1]]
        insert_heap(Point.mul_short(value))
    elif opcode == 'EcGetX':
        i = args[0][1]
        point = heap[i] 
        x, _ = point.to_affine().coordinates()
        insert_heap(x)
    elif opcode == 'EcGetY':
        i = args[0][1]
        point = heap[i] 
        _, y = point.to_affine().coordinates()
        insert_heap(y)
    elif opcode == 'PoseidonHash':
        messages = [heap[m[1]] for m in args]
        insert_heap(Base.poseidon_hash(messages))
    elif opcode == 'MerkleRoot':
        i = heap[args[0][1]]
        p = heap[args[1][1]]
        a = heap[args[2][1]]
        insert_heap(Base.merkle_root(i, p, a))
    elif opcode == 'ConstrainInstance':
        i = args[0][1]
        element = heap[i] 
        insert_publics(element)
    elif opcode == 'WitnessBase':
        type = args[0][0]
        assert type == 'Lit', f"type should LitType instead of {type}"
        print(args)
        i = args[0][1]
        element = int(literals[i][1]) # (LitType, Lit)
        base = Base.from_u64(element)
        insert_heap(base)        
    elif opcode == 'CondSelect':
        cnd = heap[args[0][1]]
        thn = heap[args[1][1]]
        els = heap[args[2][1]]
        assert cnd.eq(Base.from_u64(0)) or cnd.eq(Base.from_u64(1)), "Failed bool check"
        res = thn if cnd.eq(Base.from_u64(1)) else els
        insert_heap(res)
    elif opcode in ignored:
        print(f"Processed opcode: {opcode}")
    else:
        print(f"Missing implementation: {opcode}")

print("-------------------- END --------------------")
print(f"publics: {publics}")

################################
### Prover
################################

k = 13
zkcircuit = ZkCircuit(zkbin)


zkcircuit.witness_base(witnesses[0])
zkcircuit.witness_scalar(witnesses[1])
zkcircuit.witness_base(witnesses[2])
zkcircuit.witness_base(witnesses[3])
zkcircuit.witness_base(witnesses[4])
zkcircuit.witness_base(witnesses[5])
# zkcircuit.witness_point(witnesses[6])
# zkcircuit.witness_base(witnesses[7])
zkcircuit.witness_u32(witnesses[6])
zkcircuit.witness_merkle_path(witnesses[7])
zkcircuit.witness_base(witnesses[8])

zkcircuit = zkcircuit.build(zkbin)

print("making proving key...")
start = time()
proving_key = ProvingKey.build(k, zkcircuit)
print(f"time {time() - start}")


print("proving...")
start = time()
proof = Proof.create(proving_key, [zkcircuit], publics)
print(f"time {time() - start}")


################################
### Verifier
################################

print("building verifier circuit")
zkcircuit_v = zkcircuit.verifier_build(zkbin)

print(f"building verifying key")
start = time()
verifying_key = VerifyingKey.build(k, zkcircuit_v)
print(f"time {time() - start}")

print("verifying...")
start = time()
proof.verify(verifying_key, publics)
print(f"time {time() - start}")

print("verifying (should fail)...")
proof.verify(verifying_key, publics[:-1])