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

# Section 3.2 from "Streamlet: Textbook Streamlined Blockchains"

class Block:
	''' This class represents a tuple of the form (h, e, txs).
		Each blocks parent hash h may be computed simply as a hash of the parent block. '''
		
	def __init__(self, h, e, txs):
		self.h = h # parent hash
		self.e = e # epoch number
		self.txs = txs # transactions payload
	
	def __repr__(self):
		return "Block=[h={0}, e={1}, txs={2}]".format(self.h, self.e, self.txs)
	
	def __hash__(self):
		return hash((self.h, self.e, self.txs)) # Python hash is used for demostranation porpuses only.

class Blockchain:
	''' This class represents a sequence of blocks starting with the genesis block. '''
	
	def __init__(self, genesis_block):
		self.chain = [genesis_block]
	
	def __repr__(self):
		return "Blockchain=[chain={0}]".format(self.chain)
	
	def check_block_validity(self, block, previous_block):
		''' A block is considered valid when its parent hash is equal to the hash of the 
			previous block and their epochs are incremental, exluding genesis. '''
		
		assert(block.h != '⊥') # genesis block check
		assert(block.h == hash(previous_block))
		assert(block.e > previous_block.e)

	def check_chain_validity(self):
		''' A blockchain is considered valid, when every block is valid, based on check_block_validity method. '''
		
		for index, block in enumerate(self.chain[1:]):
			self.check_block_validity(block, self.chain[index])
	
	def add_block(self, block):		
		''' Insertion of a valid block. '''	
		self.check_block_validity(block, self.chain[-1])
		self.chain.append(block)

# We generate a genesis block and a blockchain.
genesis_block = Block("⊥", 0, '⊥')
chain = Blockchain(genesis_block)

# A new block is generated and appended to the blockchain, since its valid.
block1 = Block(hash(genesis_block), 1, "tx1, tx2, tx3")
chain.add_block(block1)

# A new block is generated and appended to the blockchain, since its valid.
block2 = Block(hash(block1), 2, "tx4, tx5, tx6")
chain.add_block(block2)

# We check entire blockchain validity.
chain.check_chain_validity()

# Following code examples will fail, due to block validity checks:
# wrong_block = Block(hash(block1), 3, "tx4,tx5,tx6") # Previous block not last.
# chain.add_block(wrong_block)

# wrong_block = Block(hash(block2), 1, "tx4,tx5,tx6") # Epoch not incremental.
# chain.add_block(wrong_block)
