# Section 3.4 from "Streamlet: Textbook Streamlined Blockchains"

from block import Block
from node import Node

# Genesis block is generated.
genesis_block = Block("⊥", 0, '⊥')

# We create some nodes to participate in the Protocol.
# There are in total n nodes numbered.
node0 = Node(0, "clock", "node_password0", genesis_block)
node1 = Node(1, "clock", "node_password1", genesis_block)
node2 = Node(2, "clock", "node_password2", genesis_block)
node3 = Node(3, "clock", "node_password3", genesis_block)
node4 = Node(4, "clock", "node_password4", genesis_block)
node5 = Node(5, "clock", "node_password5", genesis_block)

nodes = [node0, node1, node2, node3, node4, node5]

# We simulate some rounds to test consistency.
epoch = 1

# Nodes receive transactions and broacasts them between them.
# node0 receives input and broadcasts it to rest nodes.
node0.receive_transaction("tx0")
node0.broadcast_transaction([node1, node2, node3, node4, node5], "tx0")
# node1 receives input and broadcasts it to rest nodes.
node1.receive_transaction("tx2")
node1.broadcast_transaction([node0, node2, node3, node4, node5], "tx2")
# node4 receives input and broadcasts it to rest nodes.
node4.receive_transaction("tx3")
node4.broadcast_transaction([node0, node1, node2, node3, node5], "tx3")

# A random leader is selected.
leader = nodes[hash(str(epoch))%len(nodes)]

# Leader forms a block and broadcasts it.
leader.propose_block(epoch, nodes)

# Nodes vote on the block and broadcast their vote to rest nodes.
for node in nodes:
	node.vote_on_round_block(nodes)

# We verify that all nodes have the same blockchain on round end.
assert(node0.output() == node1.output() == node2.output() == node3.output() == node4.output() == node5.output())

epoch = 2

# We introduce a new node. Assumption: no history sync, a Node starts participating in next epoch.
node6 = Node(6, "clock", "node_password5", node0.output()[-1])
nodes.append(node6)

# node3 receives input and broadcasts it to rest nodes.
node3.receive_transaction("tx4")
node3.broadcast_transaction([node0, node1, node2, node4, node5, node6], "tx4")
# node5 receives input and broadcasts it to rest nodes.
node5.receive_transaction("tx5")
node5.broadcast_transaction([node0, node1, node2, node3, node4, node6], "tx5")
# node6 receives input and broadcasts it to rest nodes.
node6.receive_transaction("tx6")
node6.broadcast_transaction([node0, node1, node2, node3, node4, node5], "tx6")

# A random leader is selected.
leader = nodes[hash(str(epoch))%len(nodes)]

# Leader forms a block and broadcasts it.
leader.propose_block(epoch, nodes)

# Nodes vote on the block and broadcast their vote to rest nodes.
for node in nodes:
	node.vote_on_round_block(nodes)

# We verify that all nodes have the same blockchain on round end.
assert(node0.output() == node1.output() == node2.output() == node3.output() == node4.output() == node5.output())

# Since node6 joined later, node0 output is a prefix or equal to node6 output.
# Based on that, node6 output is a suffix of node0 output.
assert(node0.output().blocks[-len(node6.output()):] == node6.output().blocks)
print('finished...')