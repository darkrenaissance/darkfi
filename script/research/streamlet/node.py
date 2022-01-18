import utils
from block import Block
from blockchain import Blockchain
from clock import Clock
from network import Network
from vote import Vote

class Node:
	''' This class represents a protocol node.
		Each node is numbered and has a secret-public keys pair, to sign messages.
		Nodes hold a set of Blockchains(some of which are not notarized) 
		and a set of unconfirmed pending transactions. 
		All nodes have syncronized clocks, using GST approach.'''
	def __init__(self, id, network, clock, password, init_block, leader=False):
		self.id = id
		self.network=network
		self.clock = clock
		self.clock.bind(self.update_epoch)
		self.epoch = self.clock.epoch
		self.password = password
		self.private_key, self.public_key = utils.generate_keys(self.password)
		self.blockchain = Blockchain(init_block)
		self.unconfirmed_transactions = []
		self.is_leader=False
		self.leaderid=-1
	
	def __repr__(self):
		return "Node=[id={0}, clock={1}, password={2}, private_key={3}, public_key={4}, blockchain={5}, unconfirmed_transactions={6}".format(self.id, self.clock, self.password, self.private_key, self.public_key, self.blockchain, self.unconfirmed_transactions)
		
	def set_leader(self):
		self.is_leader=True

	'''
		this is a callback set by the synchronized clock
	'''
	def update_epoch(self, epoch):
		self.epoch = epoch
		#
		self.__finalize_block()
		# update blochchain
		self.update_blochchain()
		# set the new block leader
		self.set_leader()
		

	def __set_leader(self):
		self.leaderid = hash(self.epoch%self.network.num_nodes)
		if self.leaderid == self.id:
			self.is_leader=True
		else:
			self.is_leader=False

	def receive_transaction(self, transaction):
		# Additional validity rules must be defined by the protocol for its blockchain data structure.
		self.unconfirmed_transactions.append(transaction)
	
	def output(self):
		return self.blockchain
	
	def broadcast(self, nodes, transaction):
		self.network.broadcast_transaction(self.transaction)
	'''
	check if the the block are finalized to the current block
	should be called for the received blockchain
	@param n: number of the block that should be consecutive
	'''
	def is_prefix_finalized(self, n=3):
		# chain is notarized
		# the last blocks have 3 consequetive blocks indices
		# all the prefixed to block are final if it's final
		# for thre consequetive blocks in a notarized chain, all of the blocks are final up to the one before last
		L = len(self.blockchain)
		if (n>=L):
			return False
		prefix_finalized = sum([self.blockchain[L-i]==self.blockchain[L-(i+1)]+1 for i in range(1,n)])==n
		if prefix_finalized:
			for i in range(1,n+1):
				self.blockchain[L-i].set_finalized()
		return prefix_finalized

	def __finalize_block(self):
		block = Block(hash(self.blockchain.blocks[-1]), self.current_epoch, str(self.unconfirmed_transactions))
		self.blockchain.add_block(block) # Block is appended to nodes blockchain
		self.unconfirmed_transactions = []

	def update_blockchain(self):
		# only the leader need to broadcast the block
		# commit current transaction to a block, add it to the blockchain
		self.__finalize_block() 
		if self.is_leader:
			self.__broadcast_blockchain()
		else:
			#should update the blockchain
			pass
	def __vote(self):
		signed_block = utils.sign_message(self.password, self.private_key, self.blockchain)
		vote = Vote(signed_block, self.epoch, self.id)
		self.network.broadcast_vote(vote)

	def receive_vote(self, received_vote):
		# need to associate votes with block
		proposed_block_epoch = received_vote.block
		if proposed_block_epoch!=self.epoch:
			#this action depends on the policy of the p2p network
			pass
		proposed_leader_id = received_vote.id
		leader_pubkey=self.network.get_pubkey_by_id(proposed_leader_id)
		if utils.verify_signature(leader_pubkey, self.block, received_vote.vote):
			self.__vote(self)



