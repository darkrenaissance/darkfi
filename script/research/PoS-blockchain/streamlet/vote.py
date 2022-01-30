class Vote:
	''' This class represents a tuple of the form (vote, B, id). '''
	def __init__(self, vote, block, id):
		self.vote = vote # signed block
		self.block = block # epoch number
		self.id = id # node id
	
	def __repr__(self):
		return "Vote=[vote={0}, block={1}, id={2}]".format(self.vote, self.block, self.id)