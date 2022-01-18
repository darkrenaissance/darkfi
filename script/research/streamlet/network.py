'''
 this is p2p network that listens to nodes votes/broadcasts
 TODO implement
'''
from vote import  Vote
class P2pNetwork(object):
    def __P2pNetwork(self):
        #TODO implement
        # this is list of nodes with public node information
        self.nodes = []
    @property
    def num_nodes(self):
        return len(self.nodes)

    def __validate_node(self, node):
        #TODO how to verify the node
        pass
    def __validate_transaction(self, transactions):
        pass
    def __add_node(self, node):
        if not self.__validate_node(node):
            return False
        self.nodes.append(node)
        return True
    def broadcast_transaction(self, transactions, nodeid):
        if not self.__validate_transaction():
            return False
        for node in self.nodes:
            if node.id !=nodeid:
                node.recieve_transaction(transactions)
        return True        

    def broadcast_vote(self, vote):
        for node in self.nodes:
            if node.id!=vote.id:
                node.receive_vote(vote)

    def get_pubkey_by_id(self, id):
        # the current instance of the network need to fetch the public key associated from that id from the network
        # TODO how to commit that id with the pubkey? 
        return self.nodes[id].public_key