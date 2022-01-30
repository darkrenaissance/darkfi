from streamlet import Block
from ouroboros import VRF
from node import Node
import math
import numpy as np

# Genesis block is generated.
genesis_block = Block("⊥", 0, '⊥')

# We create some nodes to participate in the Protocol.
# There are in total n nodes numbered.
node0 = Node(0, "clock", "node_password0", genesis_block)
node1 = Node(1, "clock", "node_password1", genesis_block)
node4 = Node(4, "clock", "node_password4", genesis_block)

nodes = [node0, node1, node4]
# We simulate some rounds to test consistency.
epoch = 1

# Nodes receive transactions and broacasts them between them.
# node0 receives input and broadcasts it to rest nodes.
node0.receive_transaction("tx0")
node0.broadcast_transaction([node1, node4], "tx0")
# node1 receives input and broadcasts it to rest nodes.
node1.receive_transaction("tx2")
node1.broadcast_transaction([node0, node4], "tx2")
# node4 receives input and broadcasts it to rest nodes.
node4.receive_transaction("tx3")
node4.broadcast_transaction([node0, node1], "tx3")

vrf = VRF()
x = epoch
y, pi, g = vrf.sign(x)
Y = np.array(y)
y_hypotenuse2 = np.sum(Y[1]**2+Y[2]**2)
# A random leader is selected.
leader = nodes[math.ceil(y_hypotenuse2)%len(nodes)]

print(f"proposed {x}, {y}, {pi}, {vrf.pk}, {g}")
# Leader forms a block and broadcasts it.
leader.propose_block(1, y, pi, vrf.pk, g, nodes)

# Nodes vote on the block and broadcast their vote to rest nodes.
for node in nodes:
	node.vote_on_round_block(nodes)

# We verify that all nodes have the same blockchain on round end.
assert(node0.output() == node1.output() == node4.output())
