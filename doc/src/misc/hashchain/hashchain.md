# Structures 

## EventId

Hash of `Event` 

	type EventId = [u8; 32];	

## EventAction 

The `Event` could have many actions according to the underlying data.

	enum EventAction { ... };	

#### Privmsg 

| Description 	| Data Type   	| Comments																	|
|-------------- |-------------- | ------------------------------------------------------------------------- |
| nickname    	| String		| The nickname for the sender (must be less than 32 chars) 					|
| target      	| String		| The target for the `Privmsg` (recipient) 				 					|
| message     	| String		| The `Privmsg`'s content 				 									|

## Event

| Description            | Data Type      | Comments                    |
|----------------------- | -------------- | --------------------------- |
| previous_event_hash    | `EventId` 	  | Hash of the previous `Event`|
| Action     			 | `EventAction`  | `Event`'s action 			|
| Timestamp     		 | u64  		  | `Event`'s timestamp 		|
| read_confirms			 | u8	 		  | A confirmation counter 	    |

## EventNode

| Description    | Data Type      		  | Comments                    			 			  |
|--------------- | ---------------------- | ----------------------------------------------------- |
| parent    	 | Option<`EventId`> 	  | Only current root has this set to None   			  |
| Event     	 | `Event`  			  | The `Event` itself 					       			  |
| Children     	 | Vec<`EventId`>  	      | The `Event`s which has parent as this `Event` hash    |

## Model 

The `Model` consist of chains(`EventNodes`) structured as a tree, each chain has Event-based
list. To maintain strict order in chain, Each `Event` dependent on the hash of the previous `Event`. 
All the chains share a root `Event` to preserve the tree structure. 

| Description   | Data Type      		  		   | Comments                      |
|-------------- | -------------------------------- | ----------------------------- |
| current_root  | `EventId` 	  		  		   | The root `Event` for the tree |
| orphans       | HashMap<`EventId`, `Event`>  	   | Recently added `Event`s 	   |
| event_map     | HashMap<`EventId`, `EventNode`>  | The actual tree  		 	   |
| events_queue  | `EventsQueue`					   | Communication channel 

## View 

The `View` check the `Model` for new `Event`s, then dispatch these `Event`s to the clients. 

`Event`s are sorted according to the timestamp attached to each `Event`.

| Description   | Data Type      	   		    | Comments               |
|-------------- | ----------------------------- | ---------------------- |
| seen  		| HashMap<`EventId`, `Event`>   | A list of `Event`s 	 |

## EventsQueue 

The `EventsQueue` used to transport the event from `Model` to `View`.

The `Model` fill The `EventsQueue` with the new `Event`, while the `View` keep
fetching `Event`s from queue continuously.

# Architecture 

Tau using Model–view software architecture. All the operations, main data structures, 
and handling messages from network protocol, happen in the `Model` side. 
While keeping the `View` independent of the `Model` and focusing on getting update 
from it continuously.

## Add new Event

Once receiving new `Event` from the network protocol, the `Event` will be add to the
orphans list. 

After the ancestor for the new orphan gets found, The orphan `Event` will be add to the chain
according to its ancestor.

For example, in the <em> Example1 </em> below, An `Event` add to the first chain if
its previous hash is Event-A1

## Remove old leaves 

Remove leaves which are too far from the head leaf(the leaf in the longest chain).

The depth difference from the common ancestor between a leaf to be removed and a head leaf 
must be greater than `MAX_DEPTH`. 

## Update the root 

Finding the highest common ancestor for the leaves and assign it as the root
for the tree.

The highest common ancestor must have height greater than `MAX_HEIGHT`.

![data structure](../../assets/mv_event.png)

Example1




