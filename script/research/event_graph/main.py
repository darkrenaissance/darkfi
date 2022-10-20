from hashlib import sha256
from datetime import datetime
from random import randint, random
import math
import asyncio

import matplotlib.pyplot as plt
import networkx as nx


EventId = str
EventIds = list[EventId]

# Number of nodes
NODES_N = 10
# Broadcast attempt for each node
BROADCAST_ATTEMPT = 3

MAX_BROADCAST_DELAY = round(math.log(NODES_N))
MIN_BROADCAST_DELAY = 0

PROBABILITY_OF_DROPPING_MSG = 0.50  # 1/2

# Timeout for sending tasks to finish
BROADCAST_TIMEOUT = NODES_N * BROADCAST_ATTEMPT * MAX_BROADCAST_DELAY


class NetworkPool:
    def __init__(self, nodes):
        self.nodes = nodes

    def request(self, event_id: EventId):
        for n in self.nodes:
            event = n.get_event(event_id)
            if event != None:
                return event

        return None


class Event:
    def __init__(self, parents: EventIds):
        self.timestamp = datetime.now().timestamp
        self.parents = sorted(parents)

    def set_timestamp(self, timestamp):
        self.timestamp = timestamp

    # Hash of timestamp and the parents
    def hash(self) -> str:
        m = sha256()
        m.update(str.encode(str(self.timestamp)))
        for p in self.parents:
            m.update(str.encode(str(p)))
        return m.digest().hex()

    def __str__(self):

        res = f"{self.hash()}"
        for p in self.parents:
            res += f"\n    |"
            res += f"\n    - {p}"

        res += f"\n"
        return res


"""
## Graph Example
 E1: []
 E2: [E1]
 E3: [E1]
 E4: [E3]
 E5: [E3]
 E6: [E4, E5]
 E7: [E4]
 E8: [E2]
"""


class Graph:
    def __init__(self):
        self.events = dict()

    def add_event(self, event: Event):
        self.events[event.hash()] = event

    def remove_event(self, event_id: EventId):
        if event_id in self.events:
            del self.events[event_id]

    # Check if given events are exist in the graph
    # return a list of missing events
    def check(self, events: EventIds) -> EventIds:
        missing_events = []

        for e in events:
            if self.events.get(e) == None:
                missing_events.append(e)

        return missing_events

    def __str__(self):
        res = ""
        for event in self.events.values():
            res += f"\n {event}"
        return res


class Node:
    def __init__(self, name: str, queue):
        self.name = name
        self.orphan_pool = Graph()
        self.active_pool = Graph()
        self.queue = queue

        # The active pool should always start with one event
        genesis_event = Event([])
        genesis_event.set_timestamp(0.0)
        self.genesis_event = genesis_event
        self.active_pool.add_event(genesis_event)

        # On the initialization make the root node as head
        self.heads = [genesis_event.hash()]

    # Remove the parents for the event if they are exist in heads
    def remove_heads(self, event):
        for p in event.parents:
            if p in self.heads:
                self.heads.remove(p)

    # Add the event to heads
    def update_heads(self, event):
        event_hash = event.hash()
        self.remove_heads(event)
        self.heads.append(event_hash)

    # On receive new event
    def receive_new_event(self, event: Event, np):
        event_hash = event.hash()

        # Reject event with no parents
        if not event.parents:
            return

        # Reject event already exist in active pool
        if not self.active_pool.check([event_hash]):
            return

        # Reject event already exist in orphan pool
        if not self.orphan_pool.check([event_hash]):
            return

        # Check if parents for this event are missing from active pool
        missing_parents = self.active_pool.check(event.parents)

        if not missing_parents:
            # Add the event to active pool
            self.active_pool.add_event(event)
            self.update_heads(event)

            # Move events from oprhan pool to active pool if they are child of
            # the new added event
            remove_list: EventIds = []
            self.relink(event, remove_list)

            # Clean up orphan pool
            for ev in remove_list:
                self.orphan_pool.remove_event(ev)

        else:
            # Add the received event to the orphan pool
            self.orphan_pool.add_event(event)

            # Check if all missing parents are in orphan pool, otherwise
            # request them from the network
            request_list = []
            self.check_parents(request_list, missing_parents)

            print(f"{self.name} request from the network: {request_list}")

            # XXX
            # Send all the missing parents in request_list
            # to the node who send this event

            # For simulation purpose the node fetch the missed parents from the
            # network pool which contains all the nodes and its messages
            for event in request_list:
                requested_event = np.request(event)
                if requested_event != None:
                    self.receive_new_event(requested_event, np)
                else:
                    # It must always find the missed event from the network
                    print(f"Error: {self.name} requested {event} not found")

    # This will check if passed parents are in the orphan pool, and fill
    # request_list with missing parents
    def check_parents(self, request_list, parents: EventIds, visited=[]):
        for parent_hash in parents:

            # Check if the function already visit this parent
            if parent_hash in visited:
                continue
            visited.append(parent_hash)

            # If the parent in orphan pool, do recursive call to check its
            # parents as well, otherwise add the parent to request_list
            if parent_hash in self.orphan_pool.events:
                parent = self.orphan_pool.events[parent_hash]

                # Recursive call
                self.check_parents(request_list, parent.parents, visited)
            else:
                request_list.append(parent_hash)

    # Check if the orphan pool has an event linked
    # to the passed event and relink it accordingly
    def relink(self, event: Event, remove_list=[]):
        event_hash = event.hash()

        for (orphan_hash, orphan) in self.orphan_pool.events.items():

            # Check if the orphan is not already in remove_list
            if orphan_hash in remove_list:
                continue

            # Check if the event is a parent of orphan event
            if event_hash not in orphan.parents:
                continue

            # Check if the remain parents of the orphan
            # are not missing from active pool
            missing_parents = self.active_pool.check(orphan.parents)

            if not missing_parents:
                # Add the orphan to active pool
                self.active_pool.add_event(orphan)
                self.update_heads(orphan)
                # Add the orphan to remove_list
                remove_list.append(orphan_hash)

                # Recursive call
                self.relink(orphan, remove_list)

    def get_event(self, event_id: EventId):
        # Check the active_pool
        event = self.active_pool.events.get(event_id)
        # Check the orphan_pool
        if event == None:
            event = self.orphan_pool.events.get(event)

        return event

    def __str__(self):
        return f"""------
	        \n Name: {self.name}
	        \n Active Pool: {self.active_pool}
	        \n Orphan Pool: {self.orphan_pool}"""


# Each node has NODES_N of this function running in the background
# for receiving events from each node separately
async def recv_loop(node, peer, queue, np):
    while True:
        # Wait new event
        event = await queue.get()
        queue.task_done()

        if random() <= PROBABILITY_OF_DROPPING_MSG:
            print(f"{node.name} dropped: \n  {event}")
            continue

        node.receive_new_event(event, np)
        print(f"{node.name} receive event from {peer}: \n {event}")


# Send new event at random intervals
# Each node has this function running in the background
async def send_loop(node):
    for _ in range(BROADCAST_ATTEMPT):

        await asyncio.sleep(randint(MIN_BROADCAST_DELAY, MAX_BROADCAST_DELAY))

        # Create new event with the last heads as parents
        event = Event(node.heads)

        print(f"{node.name} broadcast event: \n {event}")
        for _ in range(NODES_N):
            await node.queue.put(event)
            await node.queue.join()


async def main():
    nodes = []

    print(f"Run {NODES_N} Nodes")

    try:

        # Initialize NODES_N nodes
        for i in range(NODES_N):
            queue = asyncio.Queue()
            node = Node(f"Node{i}", queue)
            nodes.append(node)

        # Initialize NetworkPool contains all nodes
        np = NetworkPool(nodes)

        # Initialize NODES_N * NODES_N coroutine tasks for receiving events
        # Each node listen to all queues from the running nodes
        for node in nodes:
            for n in nodes:
                asyncio.create_task(recv_loop(node, n.name, n.queue, np))

        # Create coroutine task contains send_loop function for each node
        # Run and wait for send tasks
        s_g = asyncio.gather(*[send_loop(n) for n in nodes])
        await asyncio.wait_for(s_g, BROADCAST_TIMEOUT)

        # Assert if all nodes share the same active pool graph
        # assert (all(n.active_pool.events.keys() ==
        #        nodes[0].active_pool.events.keys() for n in nodes))

        # Assert if all nodes share the same orphan pool graph
        # assert (all(n.orphan_pool.events.keys() ==
        #        nodes[0].orphan_pool.events.keys() for n in nodes))

        # Assert if all nodes heads are equal
        #assert (all(n.heads == nodes[0].heads for n in nodes))

        # print_graph([nodes[0]])

    except KeyboardInterrupt:
        print("Done")
    except asyncio.exceptions.TimeoutError:
        print("Broadcast TimeoutError")


def print_graph(nodes):
    for (i, node) in enumerate(nodes):
        graph = nx.Graph()

        for (h, ev) in node.active_pool.events.items():
            graph.add_node(h[:5])
            graph.add_edges_from([(h[:5], p[:5]) for p in ev.parents])

        colors = []

        node_heads = [h[:5] for h in node.heads]

        for n in graph.nodes():
            if n == "8aed6":
                colors.append("red")
            elif n in node_heads:
                colors.append("yellow")
            else:
                colors.append("blue")

        plt.figure(i)
        nx.draw_networkx(graph, with_labels=True, node_color=colors)

    plt.show()


if __name__ == "__main__":
    asyncio.run(main())
