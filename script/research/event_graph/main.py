from hashlib import sha256
from datetime import datetime
from random import randint
import asyncio

import matplotlib.pyplot as plt
import networkx as nx


EventId = str
EventIds = list[EventId]


class Event:
    def __init__(self, parents: EventIds):
        self.timestamp = datetime.now().timestamp
        self.parents = sorted(parents)

    def set_timestamp(self, timestamp):
        self.timestamp = timestamp

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

    # check if given events are exist in the graph
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
    def __init__(self, name: str):
        self.name = name
        self.orphan_pool = Graph()
        self.active_pool = Graph()

        # the active pool should always start with one event
        genesis_event = Event([])
        genesis_event.set_timestamp(0.0)

        # make the root node as head
        self.heads = [genesis_event.hash()]

        self.genesis_event = genesis_event
        self.active_pool.add_event(genesis_event)

    def remove_heads(self, event):
        for p in event.parents:
            if p in self.heads:
                self.heads.remove(p)

    def update_heads(self, event):
        event_hash = event.hash()
        self.remove_heads(event)
        self.heads.append(event_hash)

    def receive_new_event(self, event: Event):
        event_hash = event.hash()

        # reject events with no parents
        if not event.parents:
            return

        # reject event already exist in active pool
        if not self.active_pool.check([event_hash]):
            return

        # reject event already exist in orphan pool
        if not self.orphan_pool.check([event_hash]):
            return

        missing_parents = self.active_pool.check(event.parents)

        if not missing_parents:
            # if there are no missing parents
            # add the event to active pool
            self.active_pool.add_event(event)
            self.update_heads(event)

            # events list to be removed from orphan pool
            # after relink
            remove_list: EventIds = []
            self.relink(event, remove_list)

            # clean up orphan pool
            for ev in remove_list:
                self.orphan_pool.remove_event(ev)

        else:
            # add the received event to the orphan pool
            self.orphan_pool.add_event(event)

            # check if all the missing parents are in orphan pool
            # if the missing parents and their links not in orphan pool, request
            # them from the network
            request_list = []
            self.check_parents(request_list, missing_parents)

            print(f"{self.name} request from the network: {request_list}")

            # XXX
            # send all the missing parents in request_list
            # to the node who send this event

    def check_parents(self, request_list, parents: EventIds, visited=[]):
        for parent_hash in parents:
            if parent_hash in visited:
                continue

            visited.append(parent_hash)

            if parent_hash in self.orphan_pool.events:
                parent = self.orphan_pool.events[parent_hash]

                # recursive call
                self.check_parents(request_list, parent.parents, visited)
            else:
                request_list.append(parent_hash)

    def relink(self, event: Event, remove_list=[]):
        event_hash = event.hash()

        # check if the orphan pool has an event linked
        # to the new added event
        for (orphan_hash, orphan) in self.orphan_pool.events.items():
            if orphan_hash in remove_list:
                continue

            if event_hash not in orphan.parents:
                continue

            missing_parents = self.active_pool.check(orphan.parents)

            if not missing_parents:
                self.active_pool.add_event(orphan)
                self.update_heads(orphan)
                remove_list.append(orphan_hash)

                # recursive call
                self.relink(orphan, remove_list)

    def __str__(self):
        return f"""------
	        \n Name: {self.name}
	        \n Active Pool: {self.active_pool}
	        \n Orphan Pool: {self.orphan_pool}"""


MAX_BROADCAST_DELAY = 2
MIN_BROADCAST_DELAY = 0

NODES_N = 15
BROADCAST_ATTEMPT = 3
BROADCAST_TIMEOUT = NODES_N * BROADCAST_ATTEMPT * MAX_BROADCAST_DELAY


async def recv_loop(node, peer, queue):
    while True:
        event = await queue.get()
        node.receive_new_event(event)
        queue.task_done()
        print(f"{node.name} receive event from {peer}: \n {event}")


async def send_loop(node, queue):
    for _ in range(BROADCAST_ATTEMPT):

        await asyncio.sleep(randint(MIN_BROADCAST_DELAY, MAX_BROADCAST_DELAY))
        event = Event(node.heads)

        print(f"{node.name} broadcast event: \n {event}")
        for _ in range(NODES_N):
            await queue.put(event)
            await queue.join()


async def main():
    send_tasks = []
    recv_tasks = []

    nodes = []
    queues = dict()

    print(f"Run {NODES_N} Nodes")

    for i in range(NODES_N):
        node = Node(f"Node{i}")
        nodes.append(node)

        queue = asyncio.Queue()
        queues[node.name] = queue

        send_task = asyncio.create_task(send_loop(node, queue))
        send_tasks.append(send_task)

    for node in nodes:
        for (peer, queue) in queues.items():
            recv_task = asyncio.create_task(recv_loop(node, peer, queue))
            recv_tasks.append(recv_task)

    try:
        # run recv tasks
        r_g = asyncio.gather(*send_tasks)

        # run and wait for send tasks
        s_g = asyncio.gather(*send_tasks)
        await asyncio.wait_for(s_g, BROADCAST_TIMEOUT)

        # cancel recv tasks
        r_g.cancel()

        # assert if all nodes share the same active pool graph
        assert (all(n.active_pool.events.keys() ==
                nodes[0].active_pool.events.keys() for n in nodes))

        # assert if all nodes share the same orphan pool graph
        assert (all(n.orphan_pool.events.keys() ==
                nodes[0].orphan_pool.events.keys() for n in nodes))

        print_graph([nodes[0]])

    except asyncio.exceptions.TimeoutError:
        print("Broadcast TimeoutError")


def print_nodes(nodes):
    for node in nodes:
        print(node.name)
        print(len(node.active_pool.events))
        print(len(node.orphan_pool.events))
        print("###############")


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


def test_node():
    node_a = Node("NodeA")

    event0 = node_a.genesis_event
    event1 = Event([event0.hash()])
    event2 = Event([event1.hash()])
    event3 = Event([event2.hash(), event0.hash()])
    event4 = Event([event1.hash(), event3.hash()])
    event5 = Event([event4.hash(), "FAKEHASH"])
    event6 = Event([event5.hash(), event3.hash()])

    node_a.receive_new_event(event3)
    node_a.receive_new_event(event2)
    node_a.receive_new_event(event1)
    node_a.receive_new_event(event5)
    node_a.receive_new_event(event6)
    node_a.receive_new_event(event4)

    print(node_a)


if __name__ == "__main__":

    # test_node()
    asyncio.run(main())
