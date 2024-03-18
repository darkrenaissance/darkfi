import hashlib

# SMT non-membership check
# https://hackmd.io/@aztec-network/ryJ8wxfKK

# We set this to all 
NULL = bytearray([0xff] * 32)

def from_value(value):
    return value.to_bytes(32, 'little')

def hash_node(left, right):
    return hashlib.sha256(left + right).digest()

#               R
#             /   \
#         o           o             i = 2
#      /     \      /   \
#     o       o    o     o          i = 1
#    / \    /  \  /  \  /  \
#  110  4  77  5  -  -  6  -        i = 0
#    0  1   2  3  4  5  6  7

# root    0
# (2, 0)  1
# (2, 1)  2
# (1, 0)  3
# (1, 1)  4
# (1, 2)  5
# (1, 3)  6
# (0, 0)  7
# (0, 1)  8
# (0, 2)  9
# (0, 3)  10
# (0, 4)  11
# (0, 5)  12
# (0, 6)  13
# (0, 7)  14

empties = [
    NULL,
    hash_node(NULL, NULL),
]
empties.append(hash_node(empties[-1], empties[-1]))
# root for an empty tree
empties.append(hash_node(empties[-1], empties[-1]))

# Positions 4, 5 and 7 are empty
layer_0 = [
    from_value(110),
    from_value(4),
    from_value(77),
    from_value(5),
    NULL,
    NULL,
    from_value(6),
    NULL,
]

layer_1 = [
    hash_node(layer_0[0], layer_0[1]),
    hash_node(layer_0[2], layer_0[3]),
    # Subtree has empty leaves so just use NULL instead
    hash_node(layer_0[4], layer_0[5]),
    hash_node(layer_0[6], layer_0[7]),
]

layer_2 = [
    hash_node(layer_1[0], layer_1[1]),
    hash_node(layer_1[2], layer_1[3]),
]

root = hash_node(layer_2[0], layer_2[1])

table = {
    # root
    0: root,

    # (2, 0)
    1: layer_2[0],
    # This subtree contains a single value
    # (2, 1)
    2: layer_2[1],

    # (1, 0)
    3: layer_1[0],
    # (1, 1)
    4: layer_1[1],
    # ... (1, 2) (idx=5) is not needed
    # (1, 3)
    6: layer_1[3],

    # Don't add (0, 4), (0, 5) and (0, 7)
    7:  layer_0[0],
    8:  layer_0[1],
    9:  layer_0[2],
    10: layer_0[3],
    13: layer_0[6],
}

# Prove that leaf 5 is set to None
leaf = NULL
pos = 0b101

path = []
# Sibling of 5 is pos=4, with loc=(0, 4), which is idx=11
sibling_idx = 11
assert sibling_idx not in table
path.append(empties[0])
# Next node is (1, 2) with sibling (1, 3) and idx=6
path.append(table[6])
# Next node is (2, 1) with sibling (2, 0) and idx=1
path.append(table[1])

assert path == [
    NULL,
    layer_1[3],
    layer_2[0]
]
assert hash_node(path[2], hash_node(hash_node(path[0], leaf), path[1])) == root
# Starting from the bottom here

# Now do verification
assert leaf == NULL
bits = [bool(pos & (1<<n)) for n in range(3)]
node = leaf
for bit, other_node in zip(bits, path):
    nodes = (other_node, node) if bit else (node, other_node)
    node = hash_node(*nodes)
assert root == node
assert pos == 5
print("Passed")

