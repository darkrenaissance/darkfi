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

# Section 2 from "Streamlet: Textbook Streamlined Blockchains"

class Node:
	''' This class represents a simplyfied protocol node.
		Each node is numbered and has a secret-public keys pair, to sign messages. 
		Modes receive inputs (transactions) and maintain an ordered log (blockchain), 
		containing a sequense of strings (blocks). '''
		
	def __init__(self, id, secret_key, public_key):
		self.id = id
		self.secret_key = secret_key
		self.public_key = public_key
		self.blockchain = Blockchain()
		self.inputs = []
	
	def __repr__(self):
		return "Node=[id={0}, secret_key={1}, public_key={2}, blockchain={3}, inputs={4}".format(self.id, self.secret_key, self.public_key, self.blockchain, self.inputs)
		
	def receive_input(self, input):
		# Additional validity rules must be defined by the protocol for its blockchain data structure.
		self.inputs.append(input)
	
	def output(self):
		return self.blockchain
	
	def broadcast(self, nodes, input):
		for node in nodes:
			node.receive_input(input)
			
	def finalize_block(self):
		block = Block(self.inputs)
		self.blockchain.add_block(block) # Block is appended to nodes blockchain
		self.inputs = []
		
class Block:
	''' This class represents a simplyfied block structure. '''
	
	def __init__(self, transactions):
		self.transactions = transactions
	
	def __repr__(self):
		return "Block=[transactions={0}]".format(self.transactions)
	
	def __eq__(self, other):
		return self.transactions == other.transactions
		
class Blockchain:
	''' This class represents a simplyfied blockchain structure. '''
	
	def __init__(self):
		self.blocks = []
	
	def __repr__(self):
		return "Blockchain=[blocks={0}]".format(self.blocks)
	
	def __eq__(self, other):
		return self.blocks == other.blocks
		
	def __len__(self):
		return len(self.blocks)
		
	def __getitem__(self, index):
		  return self.blocks[index]
	
	def add_block(self, block):
		self.blocks.append(block)

# There are in total n nodes numbered.
node0 = Node(0, "dummy_secret_key0", "dummy_public_key0")
node1 = Node(1, "dummy_secret_key1", "dummy_public_key1")

# Advesary chooses last node to corrupt(static corruption).
corruptedNode = Node(2, "dummy_secret_key2", "dummy_public_key2")

# We simulate some rounds to test consistency.

# Round 0 synchronization period.
# node0 receives input and broadcasts it to rest nodes.
node0.receive_input("tx0")
node0.broadcast([node1, corruptedNode], "tx0")

# node1 receives input and broadcasts it to rest nodes.
node1.receive_input("tx1")
node1.broadcast([node0, corruptedNode], "tx1")

# corruptedNode receives input but doesn't broadcast to rest nodes.
corruptedNode.receive_input("tx2")

# We assume nodes finalize blocks(append to blockchain) at the end of each round.
node0.finalize_block()
node1.finalize_block()
corruptedNode.finalize_block()

# In round 1, a new node joins.
node3 = Node(3, "dummy_secret_key3", "dummy_public_key3")

# node3 receives input and broadcasts it to rest nodes.
node3.receive_input("tx3")
node3.broadcast([node0, node1, corruptedNode], "tx3")

# Nodes finalize blocks.
node0.finalize_block()
node1.finalize_block()
corruptedNode.finalize_block()
node3.finalize_block()

# Consistency testing.
# node0 and node1 remained honest, therefore their outputs must be the same.
assert(node0.output() == node1.output())

# Since node3 joined later, node0 and node1 outputs are a prefix or equal to node3 output.
# Based on that, node3 output is a suffix of node0 and node1 outputs.
assert(node0.output()[-len(node3.output()):] == node3.output().blocks)
assert(node1.output()[-len(node3.output()):] == node3.output().blocks)

# Below assertion will fail, as corrupt node deviated from the protocol.
# assert(node0.output() == corruptedNode.output())
