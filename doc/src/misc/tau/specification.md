# Specification

## EventId

Hash of all the metadata in the event

	type EventId = [u8; 32];	

## EventAction 

The event could have many actions according to the underlying data.

	enum EventAction { ... };	

## Event

| Description            | Data Type      | Comments                    |
|----------------------- | -------------- | --------------------------- |
| previous_event_hash    | `EventId` 	  | Hash of the previous event  |
| Action     			 | `EventAction`  | event's action 				|
| Timestamp     		 | u64  		  | event's timestamp 			|

## EventNode

| Description    | Data Type      		  | Comments                    			 |
|--------------- | ---------------------- | ---------------------------------------- |
| parent    	 | Option<`EventNode`> 	  | Only current root has this set to None   |
| Event     	 | `Event`  			  | The event itself 					     |
| Children     	 | Vec<`EventNode`>  	  | The events followed this event  		 |

## Model 

| Description   | Data Type      		  		   | Comments                    |
|-------------- | -------------------------------- | --------------------------- |
| current_root  | `EventId` 	  		  		   | The root event for the tree |
| orphans       | Vec<`Event`>  		  		   | Recently added events 		 |
| event_map     | HashMap<`EventId`, `EventNode`>  | The actual tree  		 	 |

## View 

| Description   | Data Type      	   | Comments                    				|
|-------------- | -------------------- | ------------------------------------------ |
| seen  		| HashSet<`EventId`>   | A list of events have imported from Model	|



