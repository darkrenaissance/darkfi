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
| parent    	 | Option<`EventId`> 	  | Only current root has this set to None   			  |
| Event     	 | `Event`  			  | The `Event` itself 					       			  |
| Children     	 | Vec<`EventId`>  	      | The `Event`s which has parent as this `Event` hash    |

## Model 

| Description   | Data Type      		  		   | Comments                      |
|-------------- | -------------------------------- | ----------------------------- |
| current_root  | `EventId` 	  		  		   | The root `Event` for the tree |
| orphans       | HashMap<`EventId`, `Event`>  	   | Recently added `Event`s 	   |
| event_map     | HashMap<`EventId`, `EventNode`>  | The actual tree  		 	   |

## View 

| Description   | Data Type      	   | Comments                    					|
|-------------- | -------------------- | ---------------------------------------------- |
| seen  		| HashSet<`EventId`>   | A list of `Event`s have imported from Model	|

## InvId

	type InvId = u64;

## InvItem

| Description   | Data Type      	   | Comments      			    |
|-------------- | -------------------- | -------------------------- |
| Id  			| `InvId`  			   | Unique generated integer	|
| Hash  		| `EventId`   		   | Hash of the Event			|

## Inv

| Description   | Data Type      	   | Comments           		|
|-------------- | -------------------- | -------------------------- |
| Invs	  	  	| Vec<`InvItem`> 	   | A list of `InvItem`		|

## GetData

| Description   | Data Type      	   | Comments              		|
|-------------- | -------------------- | -------------------------- |
| Invs	  	    | Vec<`EventId`> 	   | A list of `EventId`   		|

## UnreadMessages

| Description | Data Type                   | Comments                                                                             |
|-------------|---------------------------- | -------------------------------------------------------------------------------------|
| Messages    | HashMap<`EventId`, `Event`> | Hold all the `Event`s that have broadcasted to other nodes but haven't confirmed yet |

## SyncEvent 

| Description | Data Type    	| Comments					 	|
|-------------|---------------- |------------------------------ |
| Leaves	  | Vec<`EventId`> 	| hash of `Event`s   			|

## Seen<ObjectId>

This used to prevent receiving duplicate Objects.
The list will contains only 2^16 ids.

| Description | Data Type      | Comments			  		   |
|-------------|--------------- |------------------------------ |
| Ids		  | Vec<ObjectId>  | Contains objects ids    	   |


## Actions types

### Privmsg 

| Description 	| Data Type   	| Comments																	|
|-------------- |-------------- | ------------------------------------------------------------------------- |
| nickname    	| String		| The nickname for the sender (must be less than 32 chars) 					|
| target      	| String		| The target for the `Privmsg` (recipient) 				 					|
| message     	| String		| The `Privmsg`'s content 				 									|




