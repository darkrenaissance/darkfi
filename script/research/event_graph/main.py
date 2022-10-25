from hashlib import sha256
from datetime import datetime
from random import randint, random
from collections import Counter
import math
import asyncio
import logging
from logging import debug, error, info

import matplotlib.pyplot as plt
import networkx as nx


EventId = str
EventIds = list[EventId]


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
        if self.events.get(event_id) != None:
            del self.events[event_id]

    # Check if given events are exist in the graph
    # return a list of missing events
    def check(self, events: EventIds) -> EventIds:
        return [e for e in events if self.events.get(e) == None]

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

    # Remove the heads if they are parents of the event
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
        debug(f"{self.name} receive event from {peer}: \n {event}")
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

        # Add the new event to the orphan pool
        self.orphan_pool.add_event(event)

        # This function is the core of syncing algorithm
        #
        # Find all the links from the new event to events in orphan pool
        # Bring these events to the active pool then add the new event
        self.relink_orphan(event, np)

    def relink_orphan(self, orphan, np):
        # Check if the parents of the orphan
        # are not missing from active pool
        missing_parents = self.active_pool.check(orphan.parents)
        if missing_parents:
            # Check the missing parents from orphan pool and sync with the 
            # network for missing ones
            self.check_and_sync(list(missing_parents), np)

            # At this stage all the missing parents must be in the orphan pool
            # The next step is to move them to active pool
            self.add_linked_events_to_active_pool(missing_parents, [])

            # Check again that the parents of the orphan are in the active pool
            missing_parents = self.active_pool.check(orphan.parents)
            assert (not missing_parents)

            # Add the event to active pool
            self.add_to_active_pool(orphan)
        else:
            self.add_to_active_pool(orphan)

        # Last stage is cleaning up the orphan pool
        self.clean_orphan_pool()

    def clean_orphan_pool(self):
        debug(f"{self.name} clean_orphan_pool()")
        while True:
            remove_list = []
            for orphan in self.orphan_pool.events.values():
                # Move the orphan to active pool if it doesn't have missing 
                # parents in active pool
                missing_parents = self.active_pool.check(orphan.parents)

                if not missing_parents:
                    remove_list.append(orphan)

            if not remove_list:
                break

            for ev in remove_list:
                self.add_to_active_pool(ev)

    def check_and_sync(self, missing_events, np):
        debug(f"{self.name} check_and_sync() {missing_events}")

        while True:
            # Check if all missing parents are in orphan pool, otherwise
            # add them to request list
            request_list = []
            self.scan_orphan_pool(request_list, missing_events, [])

            if not request_list:
                break

            missing_events = self.fetch_events_from_network(request_list, np)

    # Check the missing links inside orphan pool
    def scan_orphan_pool(self, request_list, events: EventIds, visited):
        debug(f"{self.name} check_missing_parents() {events}")
        for event_hash in events:

            # Check if the function already visit this event
            if event_hash in visited:
                continue
            visited.append(event_hash)

            # If the event in orphan pool, do recursive call to check its
            # parents as well, otherwise add the event to request_list
            event = self.orphan_pool.events.get(event_hash)
            if event == None:
                # Check first if it's not in the active pool
                if self.active_pool.events.get(event_hash) == None:
                    request_list.append(event_hash)
            else:
                # Recursive call
                # Climb up for the event parents
                self.scan_orphan_pool(request_list, event.parents, visited)

    def fetch_events_from_network(self, request_list, np):
        debug(f"{self.name} fetch_events()  {request_list}")
        # XXX
        # Send the events in request_list to the node who send this event.
        #
        # For simulation purpose the node fetch the missed events from the
        # network pool which contains all the nodes and its events
        result = []
        for p in request_list:

            debug(f"{self.name} request from the network: {p}")

            # Request from the network
            requested_event = np.request(p)
            assert (requested_event != None)

            # Add it to the orphan pool
            self.orphan_pool.add_event(requested_event)
            result.extend(requested_event.parents)

        # Return parents of requested events
        return result

    def add_linked_events_to_active_pool(self, events, visited):
        debug(f"{self.name} add_linked_events_to_active_pool()  {events}")
        for event_hash in events:
            # Check if it already visit this event
            if event_hash in visited:
                continue
            visited.append(event_hash)

            if self.active_pool.events.get(event_hash) != None:
                continue

            # Get the event from the orphan pool
            event = self.orphan_pool.events.get(event_hash)

            assert (event != None)

            # Add it to the active pool
            self.add_to_active_pool(event)

            # Recursive call
            # Climb up for the event parents
            self.add_linked_events_to_active_pool(event.parents, visited)

    def add_to_active_pool(self, event):
        # Add the event to active pool
        self.active_pool.add_event(event)
        # Update heads
        self.update_heads(event)
        # Remove event from orphan pool
        self.orphan_pool.remove_event(event.hash())

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

        if random() < podm:
            debug(f"{node.name} dropped: \n  {event}")
            continue

        node.receive_new_event(event, peer, np)


# Send new event at random intervals
# Each node has this function running in the background
async def send_loop(nodes_n, max_delay, broadcast_attempt,  node):
    for _ in range(broadcast_attempt):

        await asyncio.sleep(randint(0, max_delay))

        # Create new event with the last heads as parents
        event = Event(node.heads)

        debug(f"{node.name} broadcast event: \n {event}")
        for _ in range(nodes_n):
            await node.queue.put(event)
            await node.queue.join()


"""
Run a simulation with the provided params:
    nodes_n: number of nodes
    podm: probability of dropping events (ex: 0.30 -> %30)
    broadcast_attempt: number of events each node should broadcast
    check: check if all nodes have the same graph
"""
async def run(nodes_n=3, podm=0.30, broadcast_attempt=3, check=False):

    debug(f"Running simulation with nodes: {nodes_n}, podm: {podm},\
                  broadcast_attempt: {broadcast_attempt}")

    max_delay = round(math.log(nodes_n))
    broadcast_timeout = nodes_n * broadcast_attempt * max_delay

    nodes = []

    info(f"Run {nodes_n} Nodes")

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
                debug(node)

            # Assert if all nodes share the same active pool graph
            assert (all(n.active_pool.events.keys() ==
                        nodes[0].active_pool.events.keys() for n in nodes))

            # Assert if all nodes share the same orphan pool graph
            assert (all(n.orphan_pool.events.keys() ==
                        nodes[0].orphan_pool.events.keys() for n in nodes))

            # Assert if all heads are equal
            assert (all(n.heads == nodes[0].heads for n in nodes))

        return nodes

    except asyncio.exceptions.TimeoutError:
        error("Broadcast TimeoutError")


async def main(sim_n=6, nodes_increase=False, podm_increase=False):
    # run the simulation `sim_n` times with increasing `podm` and `nodes_n`

    if nodes_increase:
        podm_increase = False

    # number of nodes
    nodes_n = 10 
    # probability of dropping events
    podm = 0.20
    # number of events each node should broadcast
    broadcast_attempt = 10

    sim_nodes_inc = int(nodes_n / 5)
    sim_podm_inc = podm / 5

    sim = []
    nodes_n_list = []
    events_synced = []
    podm_list = []

    podm_tmp = podm
    nodes_n_tmp = nodes_n

    for _ in range(sim_n):
        nodes = await run(nodes_n_tmp, podm_tmp, broadcast_attempt)
        sim.append(nodes)
        podm_list.append(podm_tmp)

        if nodes_increase:
            nodes_n_tmp += sim_nodes_inc

        if podm_increase:
            podm_tmp += sim_podm_inc

    for nodes in sim:
        nodes_n = len(nodes)

        nodes_n_list.append(nodes_n)

        events = Counter()
        for node in nodes:
            events.update(list(node.active_pool.events.keys()))

        # Remove the genesis event
        del events[node.genesis_event]

        expect_events_synced = (nodes_n * broadcast_attempt)
        actual_events_synced = 0
        for val in events.values():
            # if the event is fully synced with all nodes
            if val == nodes_n:
                actual_events_synced += 1

        res = (actual_events_synced * 100) / expect_events_synced

        events_synced.append(res)

        info(events)
        info(f"nodes_n: {nodes_n}")
        info(f"actual_events_synced: {actual_events_synced}")
        info(f"expect_events_synced: {expect_events_synced}")
        info(f"res: %{res}")

    logging.disable()

    if nodes_increase:
        plt.plot(nodes_n_list, events_synced)
        plt.ylim(0, 100)

        plt.title(
            f"Event Graph simulation with %{podm * 100} probability of dropping messages")

        plt.ylabel(
                f"Events sync percentage (each node broadcast {broadcast_attempt} events)")

        plt.xlabel("Number of nodes")
        plt.show()

    if podm_increase:
        plt.plot(podm_list, events_synced)
        plt.ylim(0, 100)

        plt.title(f"Event Graph simulation with {nodes_n} nodes")


        plt.ylabel(
                f"Events sync percentage (each node broadcast {broadcast_attempt} events)")
        plt.xlabel("Probability of dropping messages")
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

    #asyncio.run(run(nodes_n=3, podm=0, broadcast_attempt=3, check=True))
    asyncio.run(main(nodes_increase=True))
