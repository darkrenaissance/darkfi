from utils import state_hash

class Epoch(object):
    '''
    epoch spans R slots, 
    maximum number of block in Epoch is R
    epoch must start with gensis block B0
    '''
    def __init__(self, gensis_block, R, epoch_idx):
        self.gensis_block=gensis_block
        self.blocks = []
        self.R = R #maximum epoch legnth, and it's a fixed property of the system
        self.e = epoch_idx
        self.index=0
    @property
    def slot(self):
        return self.gensis_block.sl
    
    @property
    def length(self):
        return len(self.blocks)

    @property
    def index(self):
        return self.e

    @property
    def genesis(self):
        if self.length==0:
            return None
        return self.blocks[0]

    def add_block(self, block):
        if len(self.blocks)>0 and not block.state==state_hash(self.block[-1]):
            #TODO we dealing with corrupt stakeholder,
            # action to be taken
            # the caller of the add_block should execute (corrupt,U)
            pass
        if self.length==self.R:
            raise f"can't exceed Epoch's length: {self.length}"
        self.blocks.append(block)
    
    def __iter__(self):
        return self
    
    def __next__(self):
        for i in range(self.length):
            try:
                res=self.blocks[self.index]
            except IndexError:
                raise StopIteration
            self.index+=1
            return res