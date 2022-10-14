from hashlib import sha256
from datetime import datetime
import asyncio

EventId = str
EventIds = list[EventId]


class Event:
    def __init__(self, parents: EventIds):
        self.timestamp = datetime.now().timestamp
        self.parents = parents

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

        # NOTE: we will need to keep track of heads for creating new events.
        #       Not needed for this demo though.

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
            if e not in self.events:
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

    def receive_new_event(self, event: Event):

        # TODO: the active pool should always start with one event
        #       which is hardcoded into the software. The genesis event.
        #       Then we can remove this code below.
        # check if the event has no parents, and the active pool
        # is empty, then add the event directly to the active pool
        if len(event.parents) == 0:
            if len(self.active_pool.events) == 0:
                self.active_pool.add_event(event)
                self.relink(event)

            return

        missing_parents = self.active_pool.check(event.parents)

        if len(missing_parents) == 0:
            # if there are no missing parents
            # add the event to active pool
            self.active_pool.add_event(event)
            self.relink(event)
        else:
            # add the received event to the orphan pool
            self.orphan_pool.add_event(event)

            # check if all the missing parents are in orphan pool
            # if the missing parents and their links not in orphan pool, request
            # them from the network
            request_list = []
            self.check_parents(request_list, missing_parents)

            # XXX
            # send all the missing parents in request_list
            # to the node who send this event

    def check_parents(self, request_list, parents: EventIds):
        for parent_hash in parents:
            if parent_hash in self.orphan_pool.events:
                parent = self.orphan_pool.events[parent_hash]

                # recursive call
                self.check_parents(request_list, parent.parents)
            else:
                request_list.append(parent_hash)

    def relink(self, event: Event):
        # check if the orphan pool has an event linked
        # to the new added event
        # TODO: you cannot call this recursively.
        #       You must clear the orphan_pool before iteration, and keep
        #       track of all remaining orphans.
        #       Then add them back after the for loop is finished.
        #       You have a bool if things change:
        #
        #           is_reorganized = False
        #           remaining_orphans = []

        # while not is_reorganized:
        for (orphan_hash, orphan) in dict(self.orphan_pool.events).items():

            if event.hash() not in orphan.parents:
                continue

            missing_parents = self.active_pool.check(orphan.parents)

            if len(missing_parents) == 0:
                self.active_pool.add_event(orphan)
                # Error, you cannot do this. You will invalidate the iterator.
                self.orphan_pool.remove_event(orphan_hash)

                # is_reorganized = True

                # recursive call
                self.relink(orphan)

    def __str__(self):
        return f"""------
            \n Name: {self.name}
            \n Active Pool: {self.active_pool}
            \n Orphan Pool: {self.orphan_pool}"""


async def run_node(name):
    print(f"{name} Started")
    node = Node(name)

    print(f"{name} End")


async def main():
    tasks = await asyncio.gather(
        run_node("NodeA"),
        run_node("NodeB"),
        run_node("NodeC"))


def test_node():
    node_a = Node("NodeA")

    event0 = Event([])
    event1 = Event([event0.hash()])
    event2 = Event([event1.hash()])
    event3 = Event([event2.hash(), event0.hash()])
    event4 = Event([event1.hash(), event3.hash()])
    event5 = Event([event4.hash(), "FAKEHASH"])
    event6 = Event([event5.hash(), event3.hash()])

    node_a.receive_new_event(event0)
    node_a.receive_new_event(event3)
    node_a.receive_new_event(event2)
    node_a.receive_new_event(event1)
    node_a.receive_new_event(event5)
    node_a.receive_new_event(event6)
    node_a.receive_new_event(event4)

    print(node_a)


if __name__ == "__main__":
    test_node()
    asyncio.run(main())
