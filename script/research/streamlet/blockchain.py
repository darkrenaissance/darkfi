class Blockchain:
	''' This class represents a sequence of blocks starting with the genesis block. '''
	def __init__(self, intial_block):
		self.blocks = [intial_block]
	
	def __repr__(self):
		return "Blockchain=[blocks={0}]".format(self.blocks)
		
	def __eq__(self, other):
		return self.blocks == other.blocks
		
	def __len__(self):
		return len(self.blocks)
		
	def __getitem__(self, index):
		  return self.blocks[index]
	
	''' A block is considered valid when its parent hash is equal to the hash of the 
		previous block and their epochs are incremental, exluding genesis. 
		Aadditional validity rules can be applied. '''
	def check_block_validity(self, block, previous_block):
		assert(block.h != '⊥') # genesis block check
		assert(block.h == hash(previous_block))
		assert(block.e > previous_block.e)

	''' A blockchain is considered valid, when every block is valid, based on check_block_validity method. '''
	def check_chain_validity(self):
		for index, block in enumerate(self.blocks[1:]):
			self.check_block_validity(block, self.blocks[index])
	
	''' Insertion of a valid block. '''	
	def add_block(self, block):		
		self.check_block_validity(block, self.blocks[-1])
		self.blocks.append(block)
