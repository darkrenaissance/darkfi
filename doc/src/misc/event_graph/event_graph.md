# Event Graph

Event graph is a syncing mechanism between nodes working asynchronously.

![](event_graph.png)
 
The graph here is a DAG (Directed Acyclic Graph) in which the nodes 
(vertices) are user created and pushed events and the edges are the 
parent-child relation between the two endpoints.

The main purpose of the graph is synchronization. This allows nodes in 
the network maintain a fully synced store of objects. How those objects 
are interpreted is up to the application.

Each node is read-only and this is an append-only data structure. 
However the application may wish to prune old data from the store to 
conserve memory.

## Synchronization

When a new node joins the network it starts with a genesis event, and 
will:
1. ask for all connected peers for their unreferenced events (tips).
2. Compare received tips with local ones, identify which we are missing.
3. Request missing tips from peers.
4. Recursively request events backwards.

We always save the tree database so once we restart before next 
rotation we reload the tree and continue from where we left off 
(previous steps 1 through 4).

We stay in sync while connected by properly handling a new received 
event, we insert it into our dag and mark it as seen, this new event 
will be a new unreferenced event to be referenced by a newer event
if we for some reason didn't receive the event, we will be requesting 
it when reciveing newer events as we don't accept events unless we have 
their parents existing in our dag.

Synchronization task should start as soon as we connect to the p2p network.

## Sorting events

We perform a topological order of the dag, where we convert the dag 
into a sequence starting from the erlier event (genesis) to the later.

Since events could have multiple parents, there is no uniqe ordering of 
this dag, meaning events in the same layer could switch places in the 
resulted sequence, to overcome this we introduce timestamps as metadata 
of the events, we do `Depth First Search` (DFS) of the graph for every 
unreferenced tip to ensure visiting every event and sort them based on 
thier timestamp.

In case of a tie in timestamps we use event id to break the tie.

## Creating an Event

![](p2p-network.png)

Typically events are propagated through the network by rebroadcasting 
the received event to other connected peers.

In this example A creates a new event and boradcast it to its connected 
peers (B nodes), and those in turn rebroadcast it to their connected 
peers (C nodes), and so on, until every single node has received the 
event.
1. `Node A` creates a new event.
2. `Node A` sends `event` to $B_1, \dots, B_n$
3. For each $B_i$ in $\{B_1, \dots, B_n\}$:
    1. validate the event (is it older than genesis, time drifted, malicous or 
    not, etc..).
    2. Check if we already have the event. also check if we have all of 
    its parents.
    3. request missing parents if any and add them to the DAG.
    4. if all the checks pass we add the actual received event to the DAG.
    5. Relay the event to other peers.

## Genesis Event

All nodes start with a single hardcoded genesis event in their graph. 
The application layer should ignore this event. This serves as the 
origin event for synchronization.

