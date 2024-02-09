/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

# Section 3.4 from "Streamlet: Textbook Streamlined Blockchains"

from block import Block
from node import Node

# Genesis block is generated.
genesis_block = Block("⊥", 0, '⊥')
genesis_block.notarized = True
genesis_block.finalized = True

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

# We verify that all nodes have the same blockchain on round end.
assert(node0.output() == node1.output() == node2.output() == node3.output() == node4.output() == node5.output())

epoch = 2

# node3 receives input and broadcasts it to rest nodes.
node3.receive_transaction("tx4")
node3.broadcast_transaction([node0, node1, node2, node4, node5], "tx4")
# node5 receives input and broadcasts it to rest nodes.
node5.receive_transaction("tx5")
node5.broadcast_transaction([node0, node1, node2, node3, node4], "tx5")
# node2 receives input and broadcasts it to rest nodes.
node2.receive_transaction("tx6")
node2.broadcast_transaction([node0, node1, node3, node4, node5], "tx6")

# A random leader is selected.
leader = nodes[hash(str(epoch))%len(nodes)]

# Leader forms a block and broadcasts it.
leader.propose_block(epoch, nodes)

# We verify that all nodes have the same blockchain on round end.
assert(node0.output() == node1.output() == node2.output() == node3.output() == node4.output() == node5.output())

epoch = 3

# node3 receives input and broadcasts it to rest nodes.
node3.receive_transaction("tx7")
node3.broadcast_transaction([node0, node1, node2, node4, node5], "tx7")
# node5 receives input and broadcasts it to rest nodes.
node5.receive_transaction("tx8")
node5.broadcast_transaction([node0, node1, node2, node3, node4], "tx8")
# node2 receives input and broadcasts it to rest nodes.
node2.receive_transaction("tx9")
node2.broadcast_transaction([node0, node1, node3, node4, node5], "tx9")

# A random leader is selected.
leader = nodes[hash(str(epoch))%len(nodes)]

# Leader forms a block and broadcasts it.
leader.propose_block(epoch, nodes)

# We verify that all nodes have the same blockchain on round end.
assert(node0.output() == node1.output() == node2.output() == node3.output() == node4.output() == node5.output())

epoch = 4

# node3 receives input and broadcasts it to rest nodes.
node3.receive_transaction("tx19")
node3.broadcast_transaction([node0, node1, node2, node4, node5], "tx10")
# node5 receives input and broadcasts it to rest nodes.
node5.receive_transaction("tx11")
node5.broadcast_transaction([node0, node1, node2, node3, node4], "tx11")
# node2 receives input and broadcasts it to rest nodes.
node2.receive_transaction("tx12")
node2.broadcast_transaction([node0, node1, node3, node4, node5], "tx12")

# A random leader is selected.
leader = nodes[hash(str(epoch))%len(nodes)]

# Leader forms a block and broadcasts it.
leader.propose_block(epoch, nodes)

# We verify that all nodes have the same blockchain on round end.
assert(node0.output() == node1.output() == node2.output() == node3.output() == node4.output() == node5.output())