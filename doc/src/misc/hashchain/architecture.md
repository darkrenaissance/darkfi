
# Architecture 

Tau using Model–view software architecture. All the operations, main data structures, 
and handling messages from network protocol, happen in the `Model` side. 
While keeping the `View` independent of the `Model` and focusing on getting update 
from it continuously, and preserve and apply rules to received data.

## Model

The `Model` consist of chains(`EventNodes`) structured as a tree, each chain has Event-based
list. To maintain strict order each `Event` dependent on the hash of the previous `Event` 
in the chain's `Event`s. All the chains will shared a root `Event` to preserve the tree
structure. 

### Add new Event

On receiving new `Event` from the network protocol, the `Event` will be added to the
orphans list, then a process of reorganizing the `Event`s in orphans list will
start, if the `Model` doesn't have the ancestor `Event` it will ask the network
for the missing `Event`s, otherwise the `Event` will be added to a chain in the
model according to its ancestor. For example, in the <em> Example1 </em> below, 
if new `Event` (Event-A2) received, it will be added to the first chain.


### Clean the tree 

Before adding new `Event`, The `Model` must check the tree is clean and 
removing old forks which no longer needed and update the root accordingly. 

#### Remove old forks 

Steps:
- Find the common ancestor between the old fork and the current longest fork
- Find the depth from the common ancestor to both forks 
- Apply the condition:   
	longest_fork_depth - old_fork_depth > `MAX_DEPTH`
- remove the old fork 

In the <em> Example2 </em> below, assuming the `MAX_DEPTH` equal to 8, 
then EventNodeA, EventNodeB, and EventNodeE  will be removed from the tree when adding new `EventNode`.

#### Update the root 

Steps:
- Finding all the leaves
- Find common ancestors between the leaves and the head of the tree (The last `EventNode` in the longest fork) 
- Find the highest ancestor in ancestors list 
- Check if the height of the highest ancestor is greater than `MAX_HEIGHT` 
- Set the highest ancestor as new root
- Removing the parents of the new root

In the <em> Example2 </em> below, assuming the `MAX_DEPTH` equal to 4, and  `MAX_HEIGHT` 8
then EventNodeA, EventNodeB, EventNodeC, and EventNodeE  will be removed from the tree, 
and Event-D7 will be the new root for the tree

## View

The `View`'s responsibility is to checking the chains in the `Model` and asking for
new `Event`s, while keeping a list of `Event` ids which have been imported
previously to prevent importing the same `Event` twice, then order these `Event`s
according to the timestamp attached to each `Event`, the last step is 
dispatching these `Event`s to the clients. 


![data structure](../../assets/mv_event.png)

Example1

![data structure](../../assets/mv_event_tree.png)

Example2



