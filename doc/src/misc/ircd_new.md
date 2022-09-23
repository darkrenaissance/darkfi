# Ircd Protocol

## Structures

### Chain

All the nodes have only one chain contains all the `Privmsg`s in stricted order

| Description | Data Type    | Comments                                                       	|
|-------------|--------------|----------------------------------------------------------------- |
| Buffer      | Vec<Privmsg> | Contains the last 2^12 Confirmed `Privmsg`s                    	|
| Hashes      | Vec<String>  | Hold the hashes of all `Privmsg`s including the genesis `Privmsg`|

### UnreadMessages

Once a `Privmsg` received from the network it will be added to this list unless it has `read_confirms` above the `MAXIMUM CONFIRMATION`. 
All unread `Privmsg`s are continually sent until receiving confirmations from other nodes.  

All the `Privmsg`s will apply to these filtering rules: 
- Reject new `Privmsg` too far in the future from now (20 Minutes)
- Reject old `Privmsg` too far in the past from now (1 Hour)
- All `Privmsg`s are organized by timestamp. Older `Privmsg`s just gently expired and are then ignored.

| Description | Data Type                | Comments                                                                             |
|-------------|--------------------------|--------------------------------------------------------------------------------------|
| Messages    | HashMap<String, Privmsg> | Hold all the `Privmsg`s that have broadcasted to other nodes but haven't confirmed yet |

### SeenIds

Every message received its id will be add to this list to prevent duplicate messages.
The list will contains only 2^16 ids.

| Description | Data Type   | Comments							   |
|-------------|-------------|------------------------------------- |
| Ids		  | Vec<String> | Contains all the network message ids |

## Message types

### Privmsg 

| Description 	| Data Type   	| Comments																	|
|-------------- |-------------- | ------------------------------------------------------------------------- |
| id	  	  	| String	 	| A Hash of all metadata including the previous `Privmsg`'s id	  			|
| nickname    	| String		| The nickname for the sender (must be less than 32 chars) 					|
| target      	| String		| The target for the `Privmsg` (recipient) 				 					|
| message     	| String		| The `Privmsg`'s content 				 									|
| timestamp	  	| i64	 		| The timestamp for `Privmsg` (created by the sender)						|
| read_confirms	| u8	 		| A confirmation counter 	  												|
| prev_id	  	| String	 	| The id for the previous `Privmsg`	  										|


### Inv

On receiving a new `Privmsg`, the node must broadcast this message to all other nodes

| Description | Data Type   | Comments													|
|-------------|-------------|---------------------------------------------------------- |
| Id		  | String 		| A unique ID string   										|
| Hash	  	  | String 		| A hash of all `Privmsg`'s metadata RIPEMD-160(`Privmsg`)  |

### GetMsgs 

On receiving an `Inv` message if the node doesn't have the hash in `Inv`, 
Then the node will send back a `GetMsgs` to request the `Privmsg`

| Description | Data Type   | Comments													|
|-------------|-------------|---------------------------------------------------------- |
| Invs	  	  | Vec<String> | A list of `Privmsg` hashes  								|

### SyncHash 

Every 2 seconds each node must broadcast this message which contains the height of its chain
to ensure the chain remain roughly in sync.

| Description | Data Type   | Comments													|
|-------------|-------------|---------------------------------------------------------- |
| Height	  | u64 		| The Height of the chain  									|

### Hashes

Once receiving a `SyncHash` message, the node will send back all the missed hashes from the height in `SyncHash`

| Description | Data Type   | Comments																	|
|-------------|-------------|-------------------------------------------------------------------------- |
| Hashes	  | Vec<String> | A list of `Privmsg` hashes  												|
| Height  	  | u64			| The height of chain in which the first message in the Hashes list begin 	|
