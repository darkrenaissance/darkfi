class Block:
	''' This class represents a tuple of the form (h, e, txs).
		Each blocks parent hash h may be computed simply as a hash of the parent block. '''
	def __init__(self, h, e, txs):
		self.h = h # parent hash
		self.e = e # epoch number
		self.txs = txs # transactions payload
		self.finalized = False
	
	def __repr__(self):
		return "Block=[h={0}, e={1}, txs={2}], finalized={3}".format(self.h, self.e, self.txs, self.finalized)
	
	def __hash__(self):
		return hash((self.h, self.e, self.txs)) # python hash is used for demostranation porpuses only.
		
	def __eq__(self, other):
		return self.h == other.h and self.e == other.e and self.txs == other.txs

	def is_finalized():
		return self.finalized
	
	def set_finalized():
		return self.finalized=False