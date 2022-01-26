class Block:
	''' This class represents a tuple of the form (h, e, txs).
		Each blocks parent hash h may be computed simply as a hash of the parent block. '''
	def __init__(self, h, e, txs):
		self.h = h # parent hash
		self.e = e # epoch number
		self.txs = txs # transactions payload
		self.votes = [] # Epoch votes
		self.notarized = False # block notarization flag
		self.finalized = False # block finalization flag
	
	def __repr__(self):
		return "Block=[h={0}, e={1}, txs={2}, notarized={3}, finalized={4}]".format(self.h, self.e, self.txs, self.notarized, self.finalized)
	
	def __hash__(self):
		return hash((self.h, self.e, str(self.txs))) # python hash is used for demostranation porpuses only.
		
	def __eq__(self, other):
		return self.h == other.h and self.e == other.e and self.txs == other.txs
		
	def encode(self):
		return(("{0},{1},{2}".format(self.h, self.e, self.txs)).encode())
