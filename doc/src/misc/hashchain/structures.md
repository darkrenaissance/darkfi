# Structures 

## EventId

Hash of all the metadata in the `Event` 

	type EventId = [u8; 32];	

## EventAction 

The `Event` could have many actions according to the underlying data.

	enum EventAction { ... };	

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
| parent    	 | Option<`EventNode`> 	  | Only current root has this set to None   			  |
| Event     	 | `Event`  			  | The `Event` itself 					       			  |
| Children     	 | Vec<`EventNode`>  	  | The `Event`s which has parent as this `Event` hash    |

## Model 

| Description   | Data Type      		  		   | Comments                      |
|-------------- | -------------------------------- | ----------------------------- |
| current_root  | `EventId` 	  		  		   | The root `Event` for the tree |
| orphans       | Vec<`Event`>  		  		   | Recently added `Event`s 	   |
| event_map     | HashMap<`EventId`, `EventNode`>  | The actual tree  		 	   |

## View 

| Description   | Data Type      	   | Comments                    					|
|-------------- | -------------------- | ---------------------------------------------- |
| seen  		| HashSet<`EventId`>   | A list of `Event`s have imported from Model	|


## UnreadMessages

Once a `Event` received from the network it will be added to this list unless it has `read_confirms` above the `MAXIMUM CONFIRMATION`. 
All unread `Event`s are continually sent until receiving confirmations from other nodes.  

All the `Event`s will apply to these filtering rules: 
- Reject new `Event` too far in the future from now (20 Minutes)
- Reject old `Event` too far in the past from now (1 Hour)
- All `Event`s are organized by timestamp. Older `Event`s just gently expired and are then ignored.

| Description | Data Type                   | Comments                                                                             |
|-------------|---------------------------- | -------------------------------------------------------------------------------------|
| Messages    | HashMap<`InvItem`, `Event`> | Hold all the `Event`s that have broadcasted to other nodes but haven't confirmed yet |

## InvItem

Unique generated integer

	type InvItem = u32;	

## Inv

On receiving a new `Event`, the node must advertise its knowledge for this `Event` to confirm receipt 

| Description | Data Type   		| Comments				|
|-------------|--------------------	|---------------------- |
| Invs	  	  | Vec<`InvItem`> 		| A list of `InvItem`   |

## GetData

On receiving an `Inv` message if the client doesn't have the `InvItem`s, 
Sending back `GetData` message contain the missing `InvItem`s

| Description | Data Type   		| Comments				|
|-------------|--------------------	|---------------------- |
| Invs	  	  | Vec<`InvItem`> 		| A list of `EventId`   |

## Sync 

Every 2 seconds each client must broadcast this message which contains 
the head in the longest chain the client has to ensure the chain remain 
roughly in sync.

| Description | Data Type   | Comments					 	|
|-------------|-------------|------------------------------ |
| Head	      | `EventId` 	| head id in the longest chain  |

## Events  

This used in response to `Sync` message, sending all the children 
in the node correspond to `EventId` in `Sync` message 

| Description | Data Type    | Comments							|
|-------------|------------- |--------------------------------- |
| Events	  | Vec<`Event`> | A list of `Event`  			  	|
| Head  	  | `EventId`	 | The head in the `Sync` message 	|


## SeenEventIds

Every `Event` received its id will be add to this list to prevent receiving duplicate `Event`s.
The list will contains only 2^16 ids.

| Description | Data Type      | Comments			  		   |
|-------------|--------------- |------------------------------ |
| Ids		  | Vec<`EventId`> | Contains all the `Event`s ids |

## SeenInvIds

Every `InvItem` received its id will be add to this list to prevent receiving duplicate `InvItem`.
The list will contains only 2^16 ids.

| Description | Data Type      | Comments			  		     |
|-------------|--------------- |------------------------------   |
| Ids		  | Vec<`InvItem`> | Contains all the `InvItem`s ids |


## Actions types

### Privmsg 

| Description 	| Data Type   	| Comments																	|
|-------------- |-------------- | ------------------------------------------------------------------------- |
| nickname    	| String		| The nickname for the sender (must be less than 32 chars) 					|
| target      	| String		| The target for the `Privmsg` (recipient) 				 					|
| message     	| String		| The `Privmsg`'s content 				 									|




