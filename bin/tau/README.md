# Tau

Tasks management app using peer-to-peer network and raft consensus.  
multiple users can collaborate by working on the same tasks, and all users will have synced task list.


## Install 

```shell
% git clone https://github.com/darkrenaissance/darkfi 
% cd darkfi
% make BINS="taud tau"
% make install "BINS=taud tau" PREFIX=/home/XX/.local
```

## Usage (Local Deployment)

### Seed Node

First you must run a seed node. The seed node is a static host which nodes can
connect to when they first connect to the network. The `seed_session` simply
connects to a seed node and runs `protocol_seed`, which requests a list of
addresses from the seed node and disconnects straight after receiving them.

	in config file:

		## P2P accept address
		inbound="127.0.0.1:11001" 

Note that the above config doesn't specify an external address since the
seed node shouldn't be advertised in the list of connectable nodes. The seed
node does not participate as a normal node in the p2p network. It simply allows
new nodes to discover other nodes in the network during the bootstrapping phase.

### Inbound Node

This is a node accepting inbound connections on the network but which is not
making any outbound connections.

The external address is important and must be correct.

	in config file:
		
		## P2P accept address
		inbound="127.0.0.1:11002" 
		
		## P2P external address
		external_addr="127.0.0.1:11002"

		## Seed nodes to connect to 
		seeds=["127.0.0.1:11001"]

### Outbound Node

This is a node which has 8 outbound connection slots and no inbound connections.
This means the node has 8 slots which will actively search for unique nodes to
connect to in the p2p network.

	in config file:

		## Connection slots
		outbound_connections=5

		## Seed nodes to connect to 
		seeds=["127.0.0.1:11001"]

## Usage (CLI)

```shell
% tau --help 
```

	tau 0.3.0
	Tau cli

	USAGE:
		tau [FLAGS] [OPTIONS] [filters]... [SUBCOMMAND]

	FLAGS:
		-h, --help       Prints help information
		-V, --version    Prints version information
		-v               Increase verbosity

	OPTIONS:
		-c, --config <config>     Sets a custom config file
			--rpc <rpc-listen>    JSON-RPC listen URL [default: 127.0.0.1:11055]

	ARGS:
		<filters>...    Search criteria (zero or more)

	SUBCOMMANDS:
		add            Add a new task
		get            Get task by ID
		get-comment    Get task's comments
		get-state      Get task state
		help           Prints this message or the help of the given subcommand(s)
		list           List open tasks
		set-comment    Set comment for a task
		set-state      Set task state
		update         Update/Edit an existing task by ID

```shell
% tau help [SUBCOMMAND]
```

### Add new tasks

```shell
% tau add title1 description person1,person2 project1,project2 0405 4.74
% tau add title2 "some description" person1 project1 0805 18
% # this will prompt terminal for title
% tau add
Title: new title
% # then your system's default editor will open up and you could write some description
% # you should have/add environment variable EDITOR pointing to your favorite text editor
```
for more information:
```shell
% tau add --help
```


### List existing tasks

```shell
% tau list # or just tau
```
Output:
```text
 ID | Title     | Project           | Assigned        | Due             | Rank 
----+-----------+-------------------+-----------------+-----------------+------
 2  | title2    | project1          | person1         | Sunday 8 May    | 18 
 1  | title1    | project1,project2 | person1,person2 | Wednesday 4 May | 4.74 
 3  | new title |                   |                 |                 | 0 
```


### List tasks with filters

```shell
% tau all   		 # lists all tasks
% tau open 			 # lists currently open tasks
% tau pause 		 # lists currently paused tasks
% tau month 		 # lists tasks created at this month
% tau project:value  # lists all tasks that have "value" in their Project
% tau assign:value   # lists all tasks that have "value" in their Assign
% tau "rank>number"  # lists all tasks that have rank greater than "number"
% tau "rank<number"  # lists all tasks that have rank lesser than "number"
```

Combined filters:
```shell
% tau project:project1 assign:person2 month open
```
Output:
```text
 ID | Title  | Project  | Assigned        | Due             | Rank 
----+--------+----------+-----------------+-----------------+------
 1  | title1 | project1 | person1,person2 | Wednesday 4 May | 4.74 
```


### Update an existing task

```shell
% tau update 3 project project3 
% tau "rank<4" # qoutes are for escaping special characters
```
Output:
```text
 ID | Title     | Project  | Assigned | Due | Rank 
----+-----------+----------+----------+-----+------
 3  | new title | project3 |          |     | 0 
```


### Get/Set task state

```shell
% tau get-state 1 
```
Output:
```text
Task with id 1 is: "open"
```

```shell
% tau set-state 1 pause
% tau get-state 1 
```
Output:
```text
Task with id 1 is: "pause"
```

```shell
% tau set-state 2 stop # this will deactivate the task (task is done)
```


### Get/Set comment

```shell
% tau set-comment 1 person1 "some awesome comment"
% tau set-comment 1 person2 "other awesome comment"
% tau get-comment 1
```
Output:
```text
Comments on Task with id 1:
person1: some awesome comment
person2: other awesome comment
```


### Get a task

```shell
% tau get 1
```
Output:
```text
 Name          | Value 
---------------+--------------------------------
 ref_id        | cGw1AI7cBSdJWIqPMU8d355wRrB0qy 
 id            | 1 
 title         | title1 
 desc          | description 
 assign        | person1,person2 
 project       | project1 
 due           | Wednesday 4 May 
 rank          | 4.74 
 created_at    | 21:28 Monday 2 May 
 current_state | pause 
 comments      | person1: some awesome comment 
               | person2: other awesome comment 
------------------------------------------------------
 events  State changed to pause at 21:34 Monday 2 May 
------------------------------------------------------
```