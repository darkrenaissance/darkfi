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

import copy
import utils
from block import Block
from blockchain import Blockchain
from vote import Vote

class Node:
	''' This class represents a protocol node.
		Each node is numbered and has a secret-public keys pair, to sign messages.
		Nodes hold a set of Blockchains(some of which are not notarized)
		and a set of unconfirmed pending transactions.
		All nodes have syncronized clocks, using GST approach. '''

	def __init__(self, id, clock, password, init_block):
		self.id = id
		self.clock = clock	# Clock syncronization to be implemented.
		self.password = password
		self.private_key, self.public_key = utils.generate_keys(self.password)
		self.canonical_blockchain = Blockchain(init_block)
		self.node_blockchains = []
		self.unconfirmed_transactions = []

	def __repr__(self):
		return "Node=[id={0}]".format(self.id)

	def output(self):
		''' A nodes output is the finalized (canonical) blockchain they hold. '''
	
		return self.canonical_blockchain

	def receive_transaction(self, transaction):
		''' Node retreives a transaction and append it to the unconfirmed transactions list.
			Additional validity rules must be defined by the protocol for its blockchain data structure. '''

		self.unconfirmed_transactions.append(transaction)

	def broadcast_transaction(self, nodes, transaction):
		''' Node broadcast a transaction to provided nodes list. '''
		
		for node in nodes:
			node.receive_transaction(transaction)

	def find_longest_notarized_chain(self):
		''' Finds the longest fully notarized blockchain the node holds.'''
	
		longest_notarized_chain = self.canonical_blockchain
		length = 0
		for blockchain in self.node_blockchains:
			if blockchain.is_notarized() and len(blockchain.blocks) > length:
				longest_notarized_chain = blockchain
				length = len(blockchain.blocks)
		return longest_notarized_chain
		
	def get_unproposed_transactions(self):
		''' Node retrieves all unconfiremd transactions not proposed in previous blocks. '''
		unproposed_transactions = self.unconfirmed_transactions
		for blockchain in self.node_blockchains:
			for block in blockchain.blocks:
				for transaction in block.txs:
						if transaction in unproposed_transactions:
							unproposed_transactions.remove(transaction)
		return unproposed_transactions

	def propose_block(self, epoch, nodes):
		''' Node generates a block for that epoch, containing all uncorfirmed transactions.
			Block extends the longest notarized blockchain the node holds.
			Node signs the block, and broadcasts it to rest nodes. '''
	
		longest_notarized_chain = self.find_longest_notarized_chain()
		unproposed_transactions = self.get_unproposed_transactions()
		proposed_block = copy.deepcopy(Block(
			hash(longest_notarized_chain.blocks[-1]), epoch, unproposed_transactions))
		signed_proposed_block = copy.deepcopy(
			utils.sign_message(
				self.password,
				self.private_key,
				proposed_block))
		for node in nodes:
			node.receive_proposed_block(self.public_key, copy.deepcopy(
				proposed_block), copy.deepcopy(signed_proposed_block), nodes)

	def find_extended_blockchain(self, block):
		''' For a provided block, node searches for any blockchain that it extends.
			If a fork blockchain is not found, block is tested against the canonical blockchain. '''
	
		for blockchain in self.node_blockchains:
			if block.h == hash(
					blockchain.blocks[-1]) and block.e > blockchain.blocks[-1].e:
				return blockchain
		if block.h == hash(
				self.canonical_blockchain.blocks[-1]) and block.e > self.canonical_blockchain.blocks[-1].e:
			return self.canonical_blockchain
		return None

	def find_block(self, vote_block):
		''' Node searches it the blockchains it holds for provided block. '''
	
		for blockchain in self.node_blockchains:
			for block in reversed(blockchain.blocks):
				if vote_block == block:
					return block
		for block in reversed(self.canonical_blockchain.blocks):
			if vote_block == block:
				return block
		return None

	def extends_notarized_blockchain(self, blockchain):
		''' Node verifies if provided blockchain is notarized excluding the last block. '''
		
		for block in blockchain.blocks[:-1]:
			if not block.notarized:
				return False
		return True

	def vote_block(self, block, nodes):
		''' Given a block, node finds which blockchain it extends.
			If block extends the canonical blockchain, a new fork blockchain is created.
			Node votes on the block, only if it extends the longest notarized chain it has seen. '''
	
		blockchain = self.find_extended_blockchain(block)
		if not blockchain or blockchain is self.canonical_blockchain:
			blockchain = Blockchain(copy.deepcopy(block))
			self.node_blockchains.append(blockchain)
		else:
			blockchain.add_block(copy.deepcopy(block))

		if self.extends_notarized_blockchain(blockchain):
			signed_block = utils.sign_message(
				self.password, self.private_key, block)
			vote = Vote(signed_block, block, self.id)
			for node in nodes:
				node.receive_vote(self.public_key, vote, nodes)

	def receive_proposed_block(
			self,
			leader_public_key,
			round_block,
			signed_round_block,
			nodes):
		''' Node receives the proposed block, verifies its sender(epoch leader), and proceeds with voting on it. '''
		
		assert(
			utils.verify_signature(
				leader_public_key,
				round_block,
				signed_round_block))
		self.vote_block(round_block, nodes)

	def check_blockchain_finalization(self, block):
		''' For the provided block, node checks if the blockchain it extends can be finalized.
			Consensus finalization logic: If node has observed the notarization of 3 consecutive
			blocks in a fork chain, it finalizes (appends to canonical blockchain) all blocks up to the middle block.
			When a block gets finalized, the transactions it contains are removed from nodes unconfirmed transactions list.
			When fork chain blocks are finalized, rest fork chains not starting by those blocks are removed. '''
		
		if block in self.canonical_blockchain.blocks:
			blockchain = self.canonical_blockchain
		else:
			for node_blockchain in self.node_blockchains:
				if block in node_blockchain.blocks:
					blockchain = node_blockchain
		if blockchain and len(blockchain) > 2:
			if blockchain.blocks[-3].notarized and blockchain.blocks[-2].notarized:
				for block in blockchain.blocks[:-1]:
					block.finalized = True
					self.canonical_blockchain.blocks.append(block)
					for transaction in block.txs:
						if transaction in self.unconfirmed_transactions:
							self.unconfirmed_transactions.remove(transaction)
				for node_blockchain in self.node_blockchains:
					if node_blockchain.blocks[-len(blockchain.blocks[:-1]):] != blockchain.blocks[:-1]:
						self.node_blockchains.remove(node_blockchain)
					else:
						del node_blockchain[-len(blockchain.blocks[:-1]):]

	def receive_vote(self, node_public_key, vote, nodes):
		''' Node receives a vote for a block.
			First, sender is verified using their public key.
			Block is searched in nodes blockchains.
			If the vote wasn't received before, it is appended to block votes list.
			When a node sees 2n/3 votes for a block it notarizes it.			
			Finally, we check if the notarization of the block can finalize parent blocks
			in its blockchain. '''
	
		assert(utils.verify_signature(node_public_key, vote.block, vote.vote))
		vote_block = self.find_block(vote.block)
		if not vote_block:
			self.vote_block(copy.deepcopy(vote.block), nodes)
			return
		if vote not in vote_block.votes:
			vote_block.votes.append(vote)
		if not vote_block.notarized and len(vote_block.votes) > (2 * len(nodes) / 3):
			vote_block.notarized = True
			self.check_blockchain_finalization(vote_block)
