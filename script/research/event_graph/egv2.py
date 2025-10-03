#!/usr/bin/env python

# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2025 Dyne.org foundation
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, either version 3 of the
# License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

import hashlib
import time

EventId = str
EventIds = list[EventId]

class Header:
    def __init__(self, timestamp : float, layer: int, parents: EventIds):
        self.timestamp = timestamp
        self.layer = layer
        self.parents = parents

    def id(self):
        h = hashlib.sha256()
        h.update(str(self.timestamp).encode())
        for parent in self.parents:
            h.update(parent.encode())
        h.update(str(self.layer).encode())
        return h.hexdigest()

    def __str__(self):
        res = f"Header [\n\ttimestamp = {self.timestamp}, \n\tlayer = {self.layer}, \n\tparents = {self.parents}, \n ]"
        return res

class Event:
    def __init__(self, header: Header, content: str):
        self.header = header
        self.content = content

    def id(self):
        return self.header.id()

    def __str__(self):
        res = f"Event [\n header = {self.header},\n content = {self.content} \n]"
        return res

class EventGraph:
    def __init__(self):
        self.db : dict[EventId, Event] = {}
        self.tips : EventIds = []
        self.genesis = self.add_genesis() 

    def add_genesis(self) -> Event:
        event = Event(Header(time.time(), 0, []), "GENESIS_EVENT")
        self.add_event(event)
        return event

    def add_event(self, event: Event):
        event_id = event.id()
        self.db[event_id] = event
        for parent in event.header.parents:
            if parent in self.tips:
                self.tips.remove(parent)

        self.tips.append(event_id)

    def get_tip_events(self) -> list[Event]:
        events = []
        for tip in self.tips:
            events.append(self.db[tip])

        return events

    def find_paths_to_genesis(self, event: Event) -> list[list[Event]]:
        paths = []
        
        def traverse(curr_path: list[Event], event: Event, paths: list[list[Event]]):
            if len(event.header.parents) == 0:
                paths.append(list(curr_path))
                return

            for parent_hash in event.header.parents:
                parent_event = self.db[parent_hash]
                curr_path.append(parent_event)
                traverse(curr_path, parent_event, paths)
                curr_path.pop()

        traverse([event], event, paths)
        return paths

    def ancestors(self, event: Event) -> set[EventId] :
        visited = set()
        stack = [event]
        while stack:
            ev = stack.pop()
            for parent_hash in ev.header.parents:
                if parent_hash not in visited:
                    visited.add(parent_hash)
                    stack.append(self.db[parent_hash])

        return visited

    def least_common_ancestors(self, event1: Event, event2: Event) -> set [EventId]:

        anc_ev1 = self.ancestors(event1)
        anc_ev2 = self.ancestors(event2)

        common = anc_ev1 & anc_ev2
        lca = set(common)

        for h in common:
            ev = self.db[h]
            for parent in ev.header.parents:
                if parent in lca:
                    lca.remove(parent)

        return lca

    # given tips (representatives of the DAG we have) find all events 
    # that are not ancestors of these tips meaning the events we don't have in our dag
    def find_non_ancestor_events(self, tips: EventIds) -> set [EventId]:
        ancestor_events = set()
        for tip in tips:
            ancestor_events.add(tip)
            ancestor_events |= self.ancestors(self.db[tip])

        all_events = self.db.keys()

        return all_events - ancestor_events

    # builds a specific graph to test finding all paths, common ancestors
    def build_graph(self):
        '''
          This code builds the following graph

        Layer    3           2                    1                    0
             [Event3A]-----[Event2A]-------|
                                           |-----[Event1A]-----|
                                                    |          |
                                           ---------|          |
             [Event3B]-----[Event2B]-------|                   |
                                           |                   |
                                           |-----[Event1B]-----|-----[GENESIS]
                                                               |
             [Event3C]-----[Event2C]----|                      |
                                        |  |-----[Event1C]-----|
                                        ---|                   |
                                        |  |                   |
             [Event3D]-----[Event2D]----|  |------[Event1D]----|
        '''
        event1a = Event(Header(time.time(), 1, [self.genesis.id()]), "Event1A")
        event1b = Event(Header(time.time(), 1, [self.genesis.id()]), "Event1B")
        event1c = Event(Header(time.time(), 1, [self.genesis.id()]), "Event1C")
        event1d = Event(Header(time.time(), 1, [self.genesis.id()]), "Event1D")

        self.add_event(event1a)
        self.add_event(event1b)
        self.add_event(event1c)
        self.add_event(event1d)
 
        event2a = Event(Header(time.time(), 2, [event1a.id()]), "Event2A")
        event2b = Event(Header(time.time(), 2, [event1a.id(), event1b.id()]), "Event2B")
        event2c = Event(Header(time.time(), 2, [event1c.id(), event1d.id()]), "Event2C")
        event2d = Event(Header(time.time(), 2, [event1c.id(), event1d.id()]), "Event2D")

        self.add_event(event2a)
        self.add_event(event2b)
        self.add_event(event2c)
        self.add_event(event2d)

        event3a = Event(Header(time.time(), 3, [event2a.id()]), "Event3A")
        event3b = Event(Header(time.time(), 3, [event2b.id()]), "Event3B")
        event3c = Event(Header(time.time(), 3, [event2c.id()]), "Event3C")
        event3d = Event(Header(time.time(), 3, [event2d.id()]), "Event3D")
        
        self.add_event(event3a)
        self.add_event(event3b)
        self.add_event(event3c)
        self.add_event(event3d)

def main():
    graph = EventGraph()
    graph.build_graph()
    for event in graph.get_tip_events():
        print(f"Paths for {event.content}")
        paths = graph.find_paths_to_genesis(event)
        for path in paths:
            print("->".join([evt.content for evt in path]))

        print("\n")

    tips = graph.get_tip_events()
    print(f"Least Common Ancestors of {tips[0].content} and {tips[1].content}")
    lca = graph.least_common_ancestors(tips[0], tips[1])
    print([graph.db[hash].content for hash in lca])
    print("\n")
    
    print(f"Least Common Ancestors of {tips[1].content} and {tips[2].content}")
    lca = graph.least_common_ancestors(tips[1], tips[2])
    print([graph.db[hash].content for hash in lca])
    print("\n")

    print(f"Least Common Ancestors of {tips[2].content} and {tips[3].content}")
    lca = graph.least_common_ancestors(tips[2], tips[3])
    print([graph.db[hash].content for hash in lca])
    print("\n")

    non_ancestors = graph.find_non_ancestor_events([graph.tips[0]])
    print(f"Non Ancestors to {tips[0].content}")
    print([graph.db[hash].content for hash in non_ancestors])
    print("\n")
    
    non_ancestors = graph.find_non_ancestor_events([graph.tips[1]])
    print(f"Non Ancestors to {tips[1].content}")
    print([graph.db[hash].content for hash in non_ancestors])
    print("\n")
    
    non_ancestors = graph.find_non_ancestor_events([graph.tips[2]])
    print(f"Non Ancestors to {tips[2].content}")
    print([graph.db[hash].content for hash in non_ancestors])
    print("\n")
    
    non_ancestors = graph.find_non_ancestor_events([graph.tips[3]])
    print(f"Non Ancestors to {tips[3].content}")
    print([graph.db[hash].content for hash in non_ancestors])
    print("\n")

if __name__ == "__main__":
    main()
