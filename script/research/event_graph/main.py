from hashlib import sha256
from datetime import datetime
from random import randint, random
from collections import Counter
import math
import asyncio
import logging

import matplotlib.pyplot as plt
import networkx as nx
import numpy as np


EventId = str
EventIds = list[EventId]


class NetworkPool:
    def __init__(self, nodes):
        self.nodes = nodes

    def request(self, event_id: EventId):
        for n in self.nodes:
            event = n.get_event(event_id)
            if event != None:
                return (n.name, event)

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
# Graph Example
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
        self.heads = sorted(self.heads)

    # On receive new event
    def receive_new_event(self, event: Event, peer, np):
        logging.debug(f"{self.name} receive event from {peer}: \n {event}")
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

            logging.debug(
                f"{self.name} request from the network: {request_list}")

            # XXX
            # Send all the missing parents in request_list
            # to the node who send this event

            # For simulation purpose the node fetch the missed parents from the
            # network pool which contains all the nodes and its messages
            for event in request_list:
                peer, requested_event = np.request(event)
                if requested_event != None:
                    self.receive_new_event(requested_event, peer, np)
                else:
                    # It must always find the missed event from the network
                    logging.error(
                        f"Error: {self.name} requested {event} not found")

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
    def relink(self, event: Event, remove_list):
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
        return f"""
	        \n Name: {self.name}
	        \n Active Pool: {self.active_pool}
	        \n Orphan Pool: {self.orphan_pool}
	        \n Heads: {self.heads}"""


# Each node has nodes_n of this function running in the background
# for receiving events from each node separately
async def recv_loop(podm, node, peer, queue, np):
    while True:
        # Wait new event
        event = await queue.get()
        queue.task_done()

        if event == None:
            break

        if random() <= podm:
            logging.debug(f"{node.name} dropped: \n  {event}")
            continue

        node.receive_new_event(event, peer, np)


# Send new event at random intervals
# Each node has this function running in the background
async def send_loop(nodes_n, max_delay, broadcast_attempt,  node):
    for _ in range(broadcast_attempt):

        await asyncio.sleep(randint(0, max_delay))

        # Create new event with the last heads as parents
        event = Event(node.heads)

        logging.debug(f"{node.name} broadcast event: \n {event}")
        for _ in range(nodes_n):
            await node.queue.put(event)
            await node.queue.join()


"""
Run a simulation with the provided params:
    nodes_n: number of nodes
    podm: probability of dropping messages (ex: 0.30 -> %30)
    broadcast_attempt: number of messages each node should broadcast

"""
async def run(nodes_n=3, podm=0.30, broadcast_attempt=3, check=False):

    logging.debug(f"Running simulation with nodes: {nodes_n}, podm: {podm},\
                  broadcast_attempt: {broadcast_attempt}")

    max_delay = round(math.log(nodes_n))
    broadcast_timeout = nodes_n * broadcast_attempt * max_delay

    nodes = []

    logging.info(f"Run {nodes_n} Nodes")

    try:

        # Initialize nodes_n nodes
        for i in range(nodes_n):
            queue = asyncio.Queue()
            node = Node(f"Node{i}", queue)
            nodes.append(node)

        # Initialize NetworkPool contains all nodes
        np = NetworkPool(nodes)

        # Initialize nodes_n * nodes_n coroutine tasks for receiving events
        # Each node listen to all queues from the running nodes
        recv_tasks = []
        for node in nodes:
            for n in nodes:
                recv_tasks.append(recv_loop(podm, node, n.name, n.queue, np))

        r_g = asyncio.gather(*recv_tasks)

        # Create coroutine task contains send_loop function for each node
        # Run and wait for send tasks
        s_g = asyncio.gather(
            *[send_loop(nodes_n, max_delay, broadcast_attempt, n) for n in nodes])
        await asyncio.wait_for(s_g, broadcast_timeout)

        # Gracefully stop all receiving tasks
        for n in nodes:
            for _ in range(nodes_n):
                await n.queue.put(None)
                await n.queue.join()

        await r_g

        if check:

            for node in nodes:
                logging.debug(node)

            # Assert if all nodes share the same active pool graph
            assert (all(n.active_pool.events.keys() ==
                        nodes[0].active_pool.events.keys() for n in nodes))

            # Assert if all nodes share the same orphan pool graph
            assert (all(n.orphan_pool.events.keys() ==
                        nodes[0].orphan_pool.events.keys() for n in nodes))

        return nodes

    except asyncio.exceptions.TimeoutError:
        logging.error("Broadcast TimeoutError")


async def main():

    # run the simulation `sim_n` times with a fixed `podm`
    # and increase number of nodes by `sim_nodes_inc`
    sim = []
    sim_n = 5
    sim_nodes_inc = 2

    # number of nodes
    nodes_n = 10
    # probability of dropping messages
    podm = 0.10
    # number of messages each node should broadcast
    broadcast_attempt = 10

    for _ in range(sim_n):
        nodes = await run(nodes_n, podm, broadcast_attempt)
        sim.append(nodes)
        nodes_n += sim_nodes_inc

    nodes_n_list = []
    msgs_synced = []

    for nodes in sim:
        nodes_n = len(nodes)

        nodes_n_list.append(nodes_n)

        events = Counter()
        for node in nodes:
            events.update(list(node.active_pool.events.keys()))

        # Remove the genesis event
        del events["8aed642bf5118b9d3c859bd4be35ecac75b6e873cce34e7b6f554b06f75550d7"]

        expect_msgs_synced = (nodes_n * broadcast_attempt)
        actual_msgs_synced = 0
        for val in events.values():
            # if the event is fully synced with all nodes
            if val == nodes_n:
                actual_msgs_synced += 1

        res = (actual_msgs_synced * 100) / expect_msgs_synced 

        msgs_synced.append(res)

        logging.info(events)
        logging.info(f"nodes_n: {nodes_n}")
        logging.info(f"actual_msg_synced: {actual_msgs_synced}")
        logging.info(f"expect_msgs_synced: {expect_msgs_synced}")
        logging.info(f"res: %{res}")

    logging.disable()
    plt.plot(nodes_n_list, msgs_synced)
    plt.ylim(0, 100)
    plt.title(
        f"Event Graph simulation with %{podm * 100} probability of dropping messages")
    plt.ylabel("Events sync percentage")
    plt.xlabel("Number of nodes")
    plt.show()


def print_network_graph(nodes):
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
    logging.basicConfig(level=logging.DEBUG,
                        handlers=[logging.FileHandler("debug.log", mode="w"),
                                  logging.StreamHandler()])

    asyncio.run(main())
