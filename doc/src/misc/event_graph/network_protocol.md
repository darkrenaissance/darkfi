# Network Protocol

## Common Structures

### EventId

```rust
type EventId = [u8; 32];
```

## inv

Inventory vectors are used for notifying other nodes about objects they have or data which is being requested.

| Description   | Data Type      	   | Comments           		|
|-------------- | -------------------- | -------------------------- |
| invs	  	  	| `Vec<EventId>`       | Inventory items    		|

Upon receiving an unknown inventory object, a node will issue `getevent`.

## getevent

Requests event data from a node.

| Description   | Data Type      	   | Comments           		|
|-------------- | -------------------- | -------------------------- |
| invs	  	  	| `Vec<EventId>`       | Inventory items    		|

## event

Event object data. This is either sent when a new event is created, or in response to `getevent`.

| Description   | Data Type      	   | Comments           		|
|-------------- | -------------------- | -------------------------- |
| parents	  	| `Vec<EventId>`       | Parent events      		|
| timestamp 	| `u64`                | Event timestamp    		|
| action    	| `T`                  | Event specific data      	|

## getheads

This message is sent at fixed intervals when connecting to the network.
It uses this message to synchronize with the current network state.

Once updated, a node uses the messages above to stay synchronized.

| Description   | Data Type      	   | Comments           		|
|-------------- | -------------------- | -------------------------- |
| invs	  	  	| `Vec<EventId>`       | Inventory items    		|

