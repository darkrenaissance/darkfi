# Network Protocol

## Common Structures

### Event ID

Is [blake3::Hash](https://docs.rs/blake3/latest/blake3/struct.Hash.html) 
of the event, we use those IDs to request and reply 
events and tips (tips being childless events in the graph).

### Event

Representation of an event in the Event Graph.
This is either sent when a new event is created, or in response to `EventReq`.

| Description   | Data Type      	                | Comments           		     |
|-------------- | --------------------------------- | ------------------------------ |
| timestamp	  	| `u64`                             | Timestamp of the event    	 |
| content	  	| `Vec<u8>`                         | Content of the event    	     |
| parents	  	| `[blake3::Hash; N_EVENT_PARENTS]` | Parent nodes in the event DAG  |
| layer	  	    | `u64`                             | DAG layer index of the event   |

Events could have multiple parents, `N_EVENT_PARENTS` is the maximum 
number of parents an event could have.

Receiving an event with missing parents, the node will issue `EventReq`
requesting the missing parent from a peer.


## P2P Messages

### EventPut

This message serves as a container of the event being published on 
the network.

| Description   | Data Type      	   | Comments           		|
|-------------- | -------------------- | -------------------------- |
| EventPut	  	| `Event`              | Event data.         		|

### EventReq

Requests event data from a peer.

| Description   | Data Type      	   | Comments           		   |
|-------------- | -------------------- | ----------------------------- |
| EventReq	  	| `EventId`            | Request event using its ID.   |

### EventRep

Replys back the requested event's data.

| Description   | Data Type      	   | Comments           		|
|-------------- | -------------------- | -------------------------- |
| EventRep	  	| `Event`              | Reply event data.     		|

### TipReq

Requests tips from connected peers.
We use this message as first step into syncing asking connected peers 
for their DAG's tips.

### TipRep

Replys back our DAG tips' IDs.

| Description   | Data Type      	   | Comments      |
|-------------- | -------------------- | ------------- |
| TipRep	  	| `Vec<EventId>`       | Event IDs.    |
