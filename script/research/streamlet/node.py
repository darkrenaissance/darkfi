import utils
from block import Block
from blockchain import Blockchain

class Node:
	''' This class represents a protocol node.
		Each node is numbered and has a secret-public keys pair, to sign messages.
		Nodes hold a set of Blockchains(some of which are not notarized) 
		and a set of unconfirmed pending transactions. 
		All nodes have syncronized clocks, using GST approach.'''
	def __init__(self, id, clock, password, init_block):
		self.id = id
		self.clock = clock # Clock syncronization to be implemented.
		self.password = password
		self.private_key, self.public_key = utils.generate_keys(self.password)
		self.blockchain = Blockchain(init_block)
		self.unconfirmed_transactions = []
	
	def __repr__(self):
		return "Node=[id={0}, clock={1}, password={2}, private_key={3}, public_key={4}, blockchain={5}, unconfirmed_transactions={6}".format(self.id, self.clock, self.password, self.private_key, self.public_key, self.blockchain, self.unconfirmed_transactions)
		
	def receive_transaction(self, transaction):
		# Additional validity rules must be defined by the protocol for its blockchain data structure.
		self.unconfirmed_transactions.append(transaction)
	
	def output(self):
		return self.blockchain
	
	def broadcast(self, nodes, transaction):
		for node in nodes:
			node.receive_transaction(transaction)
			
	def finalize_block(self, epoch):
		block = Block(hash(self.blockchain.blocks[-1]), epoch, str(self.unconfirmed_transactions))
		self.blockchain.add_block(block) # Block is appended to nodes blockchain
		self.unconfirmed_transactions = []
