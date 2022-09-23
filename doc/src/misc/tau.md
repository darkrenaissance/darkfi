# Tau

Encrypted tasks management app using peer-to-peer network.  
Multiple users can collaborate by working on the same tasks, 
and all users will have synced tasks.


## Install 

```shell
% git clone https://github.com/darkrenaissance/darkfi 
% cd darkfi
% make BINS="taud tau"
% sudo make install "BINS=taud tau"
```

## Usage 

To run your own instance check [Local Deployment](#local-deployment)

```shell
% tau --help 
```
	tau 0.3.0

	USAGE:
	    tau [OPTIONS] [FILTERS]... [SUBCOMMAND]

	ARGS:
	    <FILTERS>...    Search filters (zero or more)                                 

	OPTIONS:
	    -e, --endpoint <ENDPOINT>    taud JSON-RPC endpoint [default: tcp://127.0.0.1:23330]
	    -h, --help                   Print help information
	    -v                           Increase verbosity (-vvv supported)
	    -V, --version                Print version information

	SUBCOMMANDS:
	    add        Add a new task.                                                    
	    comment    Set or Get comment for task(s)
	    export     Export tasks to a specified directory
	    help       Print this message or the help of the given subcommand(s)
	    import     Import tasks from a specified directory
	    info       Get all data about selected task(s)
	    list       List tasks
	    log        Log drawdown
	    modify     Modify/Edit an existing task
	    open       Open task(s)
	    pause      Pause task(s)
	    start      Start task(s)
	    stop       Stop task(s)
	    switch     Switch workspace

```shell
% tau [SUBCOMMAND] --help
```

### Quick start

#### Add tasks

```shell
% tau add Review tau usage desc:description	# will add a new task named
%						# "Review tau usage" with
%						# "description" in its desc filed
% tau add Second task assign:dave 	# will add a new task and assign it
%					# to "dave".
%					# Note: not having "desc:" key
% 					# will pop up your OS editor
%					# configured in \$EDITOR env var,
%					# this is recommended for
%					# formatting reasons and
%					# will be used through this demo.
% tau add Third task project:tau rank:1.1
% tau add Fourth task assign:dave project:tau due:1509 rank:2.5
% tau add Five
```


#### List tasks

```shell
% tau				# all non-stop tasks
% tau list			# all non-stop tasks
% tau 1-3			# tasks 1 to 3
% tau 1,2 state:open		# tasks 1 and 2 and if they are open
% tau rank:gt:2			# all tasks that have rank greater than 2
% tau due.not:today		# all tasks that thier due date is not today
% tau due.after:0909		# all tasks that thier due date is after September 9th
% tau assign:dave		# tasks that assign field is "dave"
```


#### Filtering tasks

Note: mod commands are: start, open, pause, stop and modify.

Note: All filters from the previous section could work with mod commands.

```shell
% tau 5 stop			# will stop task 5
% tau 1,3 start			# start 1 and 3
% tau 2 pause			# pause 2
% tau 2,4 modify due:2009	# edit due to September in tasks 2 and 4 
% tau 1-4 modify project:tau	# edit project to tau in tasks 1,2,3 and 4
% tau state:pause open		# open paused tasks
% tau 3 info			# show information about task 3 (does not modify)
```

#### Comments

```shell
% tau 1 comment "content foo bar"	# will add a comment to task 1
% tau 3 comment				# will show comments on task 3 
```

#### Log drawdown

```shell
% tau log 0922			# will list assignees of stopped tasks
% tau log 0922 [<Assignee>]	# will draw a heatmap of stopped tasks for [Assignee]
```

#### Export and Import

```shell
% tau export ~/example_dir	# will save tasks json files to the path
% tau import ~/example_dir	# will reload saved json files from the path
```


#### Switch workspace

```shell
% tau switch darkfi	# darkfi workspace needs to be configured in config file
```

## Tau Architecture 

Tau using Model–view software architecture. All the operations, main data structures, 
and handling messages from network protocol, happen in the Model side. 
While keeping the View independent of the Model and focusing on getting update 
from it continuously, and preserve and apply rules to received data.

### Model

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

### Find head event 
	TODO

### Find common ancestors
	TODO

### View

The view's responsibility is to checking the chains in the Model and asking for
new events, while keeping a list of event ids which have been imported
previously to prevent importing the same event twice, then order these events
according to the timestamp attached to each event, the last step is 
dispatching these events to the clients. 
<em> check the diagram bellow  </em>


![data structure](../assets/mv_event.png)


## Structures 

### EventId

Hash of all the metadata in the event

	type EventId = [u8; 32];	

### EventAction 

The event could have many actions according to the underlying data.

	enum EventAction { ... };	

### Event

| Description            | Data Type      | Comments                    |
|----------------------- | -------------- | --------------------------- |
| previous_event_hash    | `EventId` 	  | Hash of the previous event  |
| Action     			 | `EventAction`  | event's action 				|
| Timestamp     		 | u64  		  | event's timestamp 			|

### EventNode

| Description    | Data Type      		  | Comments                    			 |
|--------------- | ---------------------- | ---------------------------------------- |
| parent    	 | Option<`EventNode`> 	  | Only current root has this set to None   |
| Event     	 | `Event`  			  | The event itself 					     |
| Children     	 | Vec<`EventNode`>  	  | The events followed this event  		 |

### Model 

| Description   | Data Type      		  		   | Comments                    |
|-------------- | -------------------------------- | --------------------------- |
| current_root  | `EventId` 	  		  		   | The root event for the tree |
| orphans       | Vec<`Event`>  		  		   | Recently added events 		 |
| event_map     | HashMap<`EventId`, `EventNode`>  | The actual tree  		 	 |

### View 

| Description   | Data Type      	   | Comments                    				|
|-------------- | -------------------- | ------------------------------------------ |
| seen  		| HashSet<`EventId`>   | A list of events have imported from Model	|



## Tau network protocol
	TODO


## Local Deployment

### Seed Node

First you must run a seed node. The seed node is a static host which nodes can
connect to when they first connect to the network. The `seed_session` simply
connects to a seed node and runs `protocol_seed`, which requests a list of
addresses from the seed node and disconnects straight after receiving them.

	in config file:

		## P2P accept addresses
		inbound=["127.0.0.1:11001"] 

Note that the above config doesn't specify an external address since the
seed node shouldn't be advertised in the list of connectable nodes. The seed
node does not participate as a normal node in the p2p network. It simply allows
new nodes to discover other nodes in the network during the bootstrapping phase.

### Inbound Node

This is a node accepting inbound connections on the network but which is not
making any outbound connections.

The external addresses are important and must be correct.

	in config file:
		
		## P2P accept addresses
		inbound=["127.0.0.1:11002"]
		
		## P2P external addresses
		external_addr=["127.0.0.1:11002"]

		## Seed nodes to connect to 
		seeds=["127.0.0.1:11001"]

### Outbound Node

This is a node which has 8 outbound connection slots and no inbound connections.
This means the node has 8 slots which will actively search for unique nodes to
connect to in the p2p network.

	in config file:

		## Connection slots
		outbound_connections=8

		## Seed nodes to connect to 


