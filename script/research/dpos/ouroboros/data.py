import time
from ouroboros.logger import Logger

'''
\class Item is the basic item in the block data
'''
class Item(object):
    def __init__(self, data, fee=1):
        self.data = data
        self.fee = fee
        self.log = Logger(self)
    
    '''
    coffee reward for the miner
    '''
    @property
    def coffee(self):
        return self.fee

class GenesisItem(Item):
    def __init__(self, dict_data):
        self.fee=0
        Item.__init__(self, dict_data)
    
    def __getitem__(self, key):
        return self.data.get(key, '')

#TODO implement
class StateTransition(Item):
    def __init__(self, balance):
        self.balance = balance

#TODO implement
class TransitionProcessor(object):
    def __init__(self):
        pass

'''
\class Transaction coin exchange between two entities
'''
class Transaction(Item):
    def __init__(self, sndr_addr, rcvr_addr, amnt, fee=1, lock_time=time.time()):
        self.sndr_addr = sndr_addr
        self.rcvr_addr = rcvr_addr
        self.amnt = amnt
        self.lock_time = lock_time
        self.stamp = time.time()
        fee = fee
        Item.__init__(self, str(self), fee)
        self.log.info(str(self))

    def __repr__(self):
        return f'sender: {self.sndr_addr}, receiver: {self.rcvr_addr}, amount: {self.amnt}, self.lock time: {self.lock_time}'

class CoinBase(Item):
    def __init__(self):
        pass

'''
\class Data is the whole data stored in a single block, 
consist of list of Items
'''
class Data(list):

    def __init__(self, txs=[]):
        self.txs = txs        
    '''
    Pall, is the accumulated transactions fee/gas/coffee for a block 
    '''
    @property
    def coffee(self):
        pall = 0
        for item in self.txs:
            pall += item.coffee
        return pall

    def __repr__(self):
        buff = ''
        for item in self.txs:
            buff += str(item) + '\n'
        return buff

    def __len__(self):
        return len(self.txs)

    def __iter__(self):
        self.n=0
        return self

    def __next__(self):
        item  = None
        if self.n <= self.length:
            try:
                item = self.txs[self.n]
                self.n+=1
                return item
            except IndexError:
                raise StopIteration
    
    def append(self, item):
        self.txs.append(item)

    def __getitem__(self, i):
        L = len(self)
        if i >= L or i < 0:
            return None
        return self.txs[i]