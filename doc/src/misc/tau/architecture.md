
# Architecture 

Tau using Model–view software architecture. All the operations, main data structures, 
and handling messages from network protocol, happen in the Model side. 
While keeping the View independent of the Model and focusing on getting update 
from it continuously, and preserve and apply rules to received data.

## Model

The Model consist of chains(EventNodes) structured as a tree, each chain has Event-based
list. To maintain strict order each event dependent on the hash of the previous event 
in the chain's events. All the chains will shared a root event to preserve the tree
structure. <em> check the diagram bellow  </em>

On receiving new event from the network protocol, the event will be added to the
orphans list, then a process of reorganizing the events in orphans list will
start, if the Model doesn't have the ancestor event it will ask the network
for the missing events, otherwise the event will be added to a chain in the
model according to its ancestor. For example, in the diagram below, 
if new event (Event-A2) received, it will be added to the first chain.

## Find head event 
	TODO

## Find common ancestors
	TODO

## View

The view's responsibility is to checking the chains in the Model and asking for
new events, while keeping a list of event ids which have been imported
previously to prevent importing the same event twice, then order these events
according to the timestamp attached to each event, the last step is 
dispatching these events to the clients. 
<em> check the diagram bellow  </em>


![data structure](../../assets/mv_event.png)




