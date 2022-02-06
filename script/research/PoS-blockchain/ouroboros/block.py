import json
from utils import encode_genesis_data, decode_gensis_data, state_hash

'''
single block B_i for slot i in the system live time L,
assigned to stakeholder U_j, with propability P_j_i,
in the chain C, should be signed by U_j keys.
'''
class Block(object):
    '''
    @param previous_block: parent block to the current block
    @param data: is the transaction, or contracts in the leadger, or gensis block data, 
        data is expected to be binary, no format is enforced
    @param slot_uid: the block corresponding slot monotonically increasing index, 
        it's one-based
    @param gensis: boolean, True for gensis block
    '''
    def __init__(self, previous_block, data, slot_uid, genesis=False):
        # state is hte hash of the previous block in the blockchain
        self.state=''
        if slot_uid>1:
            self.state=state_hash(previous_block)
        self.tx = data
        self.sl = slot_uid
        self.is_genesis=genesis

    def __repr__(self):
        if self.is_genesis:
            return "GensisBlock at {slot:"+self.sl+",data:"+self.tx+",state:"+self.state+"}\n"+\
                decode_gensis_data(self.data)
        return "Block at {slot:"+self.sl+",data:"+self.tx+",state:"+self.state+"}"
    
    def __eq__(self, block):
        return self.state==block.state and \
            self.tx == block.tx and \
            self.sl == block.sl

    def to_json(self):
        d = {'state':self.state, \
            'data': self.tx, \
            'sl': self.sl}
        return json.encoder(d)

    @property
    def state(self):
        return self.st
    
    @property
    def data(self):
        return self.tx

    @property
    def slot(self):
        return self.sl

    @property
    def empty(self):
        return (self.data=='' or self.slot<0) and self.state==''

class GensisBlock(Block):
    '''
    @param data: data is dictionary of  list of (pk_i, s_i) public key,
        and stake respectively of the corresponding stakeholder U_i,
        seed of the leader election function.
    '''
    def __init__(self, previous_block, data, slot_uid):
        # stakeholders is list of tuple (pk_i, s_i) for the ith stakeholder
        self.stakeholders = data['stakeholders']
        self.seed = data['seed']
        data = encode_genesis_data(self.stakeholders, self.seed)
        super.__init__(previous_block, data, slot_uid, True)
    '''
    @return: the number of participating stakeholders in the blockchain
    '''
    @property
    def length(self):
        return len(self.stakeholders)

    def __getitem__(self, i):
        if i<0 or i>=self.length:
            raise "index is out of range!"
        return self.stakeholders[i]

'''
block lead by an adversary, or 
lead by offline leader
is an empty Block
'''
class EmptyBlock(Block):
    def __init__(self):
        super.__init__(None, '', -1, False)
