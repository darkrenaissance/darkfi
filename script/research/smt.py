import hashlib

# SMT non-membership check
# https://hackmd.io/@aztec-network/ryJ8wxfKK

NULL = bytearray(32)

def from_value(value):
    return value.to_bytes(32, 'little')

def hash_node(left, right):
    return hashlib.sha256(left + right).digest()

#               R
#             /   \
#         o           o
#      /     \      /   \
#     o       o    o     o
#    / \    /  \  /  \  /  \
#  110  4  77  5  -  -  6  -
#    0  1   2  3  4  5  6  7

pos = {
    110: 0,
    4: 1,
    77: 2,
    5: 3,
    6: 6
}

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
    0: (layer_2[0], layer_2[1]),

    # [1, 0]
    1: (layer_1[0], layer_1[1]),
    # This subtree contains a single value so just store that
    # [1, 1]
    2: (layer_1[2], 2, True),

    # [2, 0]
    layer_1[0]: (layer_0[0], layer_0[1]),
    # [2, 1]
    layer_1[1]: (layer_0[2], layer_0[3]),
    # ... [2, 2] and [2, 3] are not needed
}

# Prove that leaf 5 is set to None
leaf = NULL
path = [
    NULL,
    layer_1[3],
    layer_2[0]
]
assert hash_node(path[2], hash_node(hash_node(path[0], leaf), path[1])) == root
# Starting from the bottom here
pos = 0b101

# Now do verification
bits = [bool(pos & (1<<n)) for n in range(3)]
node = leaf
for bit, other_node in zip(bits, path):
    nodes = (other_node, node) if bit else (node, other_node)
    node = hash_node(*nodes)
assert root == node
print("Passed")

