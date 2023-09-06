/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

from hashlib import sha256
from random import randint, random, getrandbits
from collections import Counter
import time
import math
import asyncio
import logging
from logging import debug, error, info

import matplotlib.pyplot as plt
import networkx as nx
import numpy as np


EventId = str
EventIds = list[EventId]


def ntp_request() -> float:
    # add random clock drift
    if bool(getrandbits(1)):
        return time.time() + randint(0, 10)
    else:
        return time.time() - randint(0, 10)


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
        self.timestamp = ntp_request()
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


class Graph:
    def __init__(self, max_time_diff):
        self.events = dict()
        self.heads = []
        self.tails = []
        self.max_time_diff = max_time_diff

    def add_event(self, event: Event):
        event_id = event.hash()

        if self.events.get(event_id) != None:
            return

        self.events[event_id] = event

        self.update_heads(event)
        self.update_tails(event)

    def update_tails(self, event):
        event_hash = event.hash()

        if event_hash in self.tails:
            return

        # Remove tails if they are parents of the given event
        for p in event.parents:
            if p in self.tails:
                self.tails.remove(p)

        # Add the event to tails
        self.tails.append(event_hash)
        self.tails = sorted(self.tails)

    def update_heads(self, event):
        event_hash = event.hash()

        if event_hash in self.heads:
            return

        # Remove heads if they are parents of the given event
        for p in event.parents:
            if p in self.heads:
                self.heads.remove(p)

        # Add the event to heads
        self.heads.append(event_hash)
        self.heads = sorted(self.heads)

    # Check if the event is too old from now, by subtracting current timestamp
    # from event timestamp, it must be more than `max_time_diff' to be consider
    # old event
    def is_old_event(self, event: Event):
        # Ignore genesis event
        if event.timestamp == 0.0:
            return False

        current_timestamp = ntp_request()
        diff = current_timestamp - event.timestamp
        if diff > self.max_time_diff:
            return True
        return False

    def prune_old_events(self):
        # Find the old events
        old_events = [eh for eh, ev in self.events.items() if
                       self.is_old_event(ev)]

        # Remove the old events
        for eh in old_events:
            self.remove_event(eh)
        
    def remove_event(self, eh: EventId):
        self.events.pop(eh, None)
        # Remove old events from heads
        if eh in self.heads:
            self.heads.remove(eh)

        # Remove old events from tails
        if eh in self.tails:
            self.tails.remove(eh)

    # Check if given events are exist in the graph
    # return a list of missing events
    def check_events(self, events: EventIds) -> EventIds:
        return [e for e in events if self.events.get(e) == None]

    def __str__(self):
        res = ""
        for event in self.events.values():
            res += f"\n {event}"
        return res


class Node:
    def __init__(self, name: str, queue, max_time_diff):
        self.name = name
        self.orphan_pool = Graph(max_time_diff)
        self.active_pool = Graph(max_time_diff)
        self.queue = queue

        # Pruned events from active pool
        self.pruned_events = []

        # The active pool should always start with one event
        genesis_event = Event([])
        genesis_event.set_timestamp(0.0)
        self.active_pool.add_event(genesis_event)

    # On create new event
    def new_event(self):
        # Pruning old events from active pool
        self.active_pool.prune_old_events()
        return Event(self.active_pool.heads)

    # On receive new event
    def receive_new_event(self, event: Event, peer, np):
        debug(f"{self.name} receive event from {peer}: \n {event}")
        event_hash = event.hash()

        # Reject event with no parents
        if not event.parents:
            return

        # XXX Reject old event
        # no need for this simulation
        # if self.is_old_event(event):
        #   return

        # Reject event already exist in active pool
        if not self.active_pool.check_events([event_hash]):
            return

        # Reject event already exist in orphan pool
        if not self.orphan_pool.check_events([event_hash]):
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
        missing_parents = self.active_pool.check_events(orphan.parents)
        missing_parents = self.check_pruned_events(missing_parents)
        if missing_parents:
            # Check the missing parents from orphan pool and sync with the
            # network for missing ones
            self.check_and_sync(list(missing_parents), np)

            # At this stage all the missing parents must be in the orphan pool
            # The next step is to move them to active pool
            self.add_linked_events_to_active_pool(missing_parents, [])

            # Check again that the parents of the orphan are in the active pool
            missing_parents = self.active_pool.check_events(orphan.parents)
            missing_parents = self.check_pruned_events(missing_parents)
            assert (not missing_parents)

            # Add the event to active pool
            self.activate_event(orphan)
        else:
            self.activate_event(orphan)

        # Last stage, Cleaning up the orphan pool:
        #  - Remove orphan if it is too old according to `max_time_diff`
        #  - Move orphan to active pool if it doesn't have any missing parents
        self.clean_pools()
        

    def clean_pools(self):
        self.active_pool.prune_old_events()

        for event in self.active_pool.events.values():

            # Check if the event parents are old events
            old_parents = self.active_pool.check_events(event.parents)

            if not old_parents:
                continue

            # Add the event to tails if it has only old events as parents
            self.active_pool.update_tails(event)

        self.orphan_pool.prune_old_events()

        while True:
            active_list = []
            for orphan in self.orphan_pool.events.values():

                # Move the orphan to active pool if it doesn't have missing
                # parents in active pool
                missing_parents = self.active_pool.check_events(orphan.parents)

                if not missing_parents:
                    active_list.append(orphan)
            
            if not active_list:
                break

            for ev in active_list:
                self.activate_event(ev)
        

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
                if self.active_pool.events.get(event_hash) != None:
                    continue

                # Check if it's not in pruned events
                if event_hash in self.pruned_events:
                    continue

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

            if requested_event == None:
                if p not in self.pruned_events:
                    self.pruned_events.append(p)
                continue

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

            if event_hash in self.pruned_events:
                continue

            # Get the event from the orphan pool
            event = self.orphan_pool.events.get(event_hash)

            assert (event != None)

            # Add it to the active pool
            self.activate_event(event)

            # Recursive call
            # Climb up for the event parents
            self.add_linked_events_to_active_pool(event.parents, visited)

    def activate_event(self, event):
        # Add the event to active pool
        self.active_pool.add_event(event)
        # Remove event from orphan pool
        self.orphan_pool.remove_event(event.hash())

    # Get an event from orphan pool or active pool
    def get_event(self, event_id: EventId):
        # Check the active_pool
        event = self.active_pool.events.get(event_id)
        # Check the orphan_pool
        if event == None:
            event = self.orphan_pool.events.get(event)

        return event

    # Clean up the given events from pruned events
    def check_pruned_events(self, events):
        return [ev for ev in events if ev not in self.pruned_events]

    def __str__(self):
        return f"""
	        \n Name: {self.name}
	        \n Active Pool: {self.active_pool}
	        \n Orphan Pool: {self.orphan_pool}"""


# Each node has `nodes_n` of this function running in the background
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
        event = node.new_event()

        debug(f"{node.name} broadcast event: \n {event}")
        for _ in range(nodes_n):
            await node.queue.put(event)
            await node.queue.join()


"""
Run a simulation with the provided params:
    nodes_n: number of nodes
    podm: probability of dropping events (ex: 0.30 -> %30)
    broadcast_attempt: number of events each node should broadcast
    max_time_diff: a max difference in time to detect an old event 
    check: check if all nodes have the same graph
"""
async def run(nodes_n=3, podm=0.30, broadcast_attempt=3, max_time_diff=180.0,
              check=False, max_delay=None):

    debug(f"Running simulation with nodes: {nodes_n}, podm: {podm},\
                  broadcast_attempt: {broadcast_attempt}")

    if max_delay == None:
        max_delay = round(math.log(nodes_n))

    broadcast_timeout = nodes_n * broadcast_attempt * max_delay

    nodes = []

    info(f"Run {nodes_n} Nodes")

    try:

        # Initialize `nodes_n` nodes
        for i in range(nodes_n):
            queue = asyncio.Queue()
            node = Node(f"Node{i}", queue, max_time_diff)
            nodes.append(node)

        # Initialize NetworkPool contains all nodes
        np = NetworkPool(nodes)

        # Initialize `nodes_n` * `nodes_n` coroutine tasks for receiving events
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

        return nodes

    except asyncio.exceptions.TimeoutError:
        error("Broadcast TimeoutError")


async def main(sim_n=6, nodes_increase=False, podm_increase=False,
               time_diff_decrease=False):

    # run the simulation `sim_n` times, while enabling one of these params:
    #  - increasing `podm`
    #  - increasing `nodes_n`
    #  - decreasing `max_time_diff`

    if nodes_increase:
        podm_increase = False
        time_diff_decrease = False

    if podm_increase:
        time_diff_decrease = False

    # number of nodes
    nodes_n = 100
    # probability of dropping events
    podm = 0.0
    # a max difference in time to detect an old event
    max_time_diff = 60  # seconds
    # number of events each node should broadcast
    broadcast_attempt = 10

    # Number of nodes get increase in each simulation
    sim_nodes_inc = int(nodes_n / 5)
    # A value get add to `podm` in each simulation
    sim_podm_inc = podm / 5
    # A value get subtract from `max_time_diff` in each simulation
    sim_diff_time_dec = max_time_diff / 10

    # Contains the nodes for each simulation
    simulations = []

    # Contains the `podm` variables for each simulation
    podm_list = []

    # Contains the `max_time_diff` variables for each simulation
    mtd_list = []

    podm_tmp = podm
    nodes_n_tmp = nodes_n
    time_diff_tmp = max_time_diff

    for _ in range(sim_n):
        nodes = await run(nodes_n_tmp, podm_tmp, broadcast_attempt,
                          max_time_diff=time_diff_tmp)
        simulations.append(nodes)
        podm_list.append(podm_tmp)
        mtd_list.append(time_diff_tmp)

        if nodes_increase:
            nodes_n_tmp += sim_nodes_inc

        if podm_increase:
            podm_tmp += sim_podm_inc

        if time_diff_decrease:
            time_diff_tmp -= sim_diff_time_dec

    # Numbers of nodes for each simulation
    nodes_n_list = []
    # Synced events percentage for each simulation
    events_synced_perc = []
    # Number of events in active pool for each simulations
    active_events = []
    # Number of events in pruned events list for each simulations
    pruned_events = []

    for nodes in simulations:
        nodes_n = len(nodes)

        nodes_n_list.append(nodes_n)

        events = Counter()
        p_events = Counter()
        for node in nodes:
            events.update(list(node.active_pool.events.keys()))
            p_events.update(node.pruned_events)

        expect_events_synced = (nodes_n * broadcast_attempt) + 1

        actual_events_synced = 0
        for val in events.values():
            # If the event is fully synced with all nodes
            if val == nodes_n:
                actual_events_synced += 1

        pruned_events_synced = 0
        for val in p_events.values():
            # If the pruned event is fully synced with all nodes
            if val == nodes_n:
                pruned_events_synced += 1

        res = (actual_events_synced * 100) / expect_events_synced
        events_synced_perc.append(res)

        active_events.append(actual_events_synced)
        pruned_events.append(pruned_events_synced)

        info(f"nodes_n: {nodes_n}")
        info(f"actual_events_synced: {actual_events_synced}")
        info(f"expect_events_synced: {expect_events_synced}")
        info(f"pruned_events_synced: {pruned_events_synced}")
        info(f"res: %{res}")

    # Disable logging for matplotlib
    logging.disable()

    if nodes_increase:
        plt.plot(nodes_n_list, events_synced_perc)
        plt.ylim(0, 100)

        plt.title(
            f"Event Graph simulation with %{podm * 100} probability of dropping messages")

        plt.ylabel(
            f"Events sync percentage (each node broadcast {broadcast_attempt} events)")

        plt.xlabel("Number of nodes")
        plt.show()
        return

    if podm_increase:
        plt.plot(podm_list, events_synced_perc)
        plt.ylim(0, 100)

        plt.title(f"Event Graph simulation with {nodes_n} nodes")

        plt.ylabel(
            f"Events sync percentage (each node broadcast {broadcast_attempt} events)")
        plt.xlabel("Probability of dropping messages")
        plt.show()
        return

    if time_diff_decrease:

        x = np.arange(len(mtd_list))  # the label locations
        width = 0.35  # the width of the bars

        fig, ax = plt.subplots()
        rects1 = ax.bar(x - width/2, active_events, width, label='Active')
        rects2 = ax.bar(x + width/2, pruned_events, width, label='Pruned')

        plt.title(
            f"Event Graph simulation with %{podm * 100} probability of dropping messages, and {nodes_n} nodes")

        plt.ylabel(
            f"Number of events broadcasted during the simulation (each node broadcast {broadcast_attempt} events)")

        plt.xlabel("A time duration to detect old events (in seconds)")

        plt.ylim(0, (broadcast_attempt * nodes_n) + 1)
        ax.set_xticks(x, mtd_list)
        ax.legend()

        ax.bar_label(rects1, padding=3)
        ax.bar_label(rects2, padding=3)

        fig.tight_layout()

        plt.show()
        return


def print_network_graph(node, unpruned=False):
    logging.disable()
    graph = nx.Graph()

    if unpruned:
        for (h, ev) in node.unpruned_active_pool.events.items():
            graph.add_node(h[:5])
            graph.add_edges_from([(h[:5], p[:5]) for p in ev.parents])
    else:
        for (h, ev) in node.active_pool.events.items():
            graph.add_node(h[:5])
            graph.add_edges_from([(h[:5], p[:5]) for p in ev.parents])

    colors = []

    for n in graph.nodes():
        if any(n == t[:5] for t in node.tails):
            colors.append("red")
        elif any(n == h[:5] for h in node.heads):
            colors.append("yellow")
        else:
            colors.append("#697aff")

    nx.draw_networkx(graph, with_labels=True, node_color=colors)

    plt.show()


if __name__ == "__main__":
    logging.basicConfig(level=logging.DEBUG,
                        handlers=[logging.FileHandler("debug.log", mode="w"),
                                  logging.StreamHandler()])

    #nodes = asyncio.run(run(nodes_n=14, podm=0, broadcast_attempt=4,
    #                    max_time_diff=30,check=True))

    # print_network_graph(nodes[0])
    # print_network_graph(nodes[0], unpruned=True)

    # asyncio.run(main(time_diff_decrease=True))
