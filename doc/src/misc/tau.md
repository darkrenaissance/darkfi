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
	tau 0.4.1

	Usage: tau [OPTIONS] [FILTERS]... [COMMAND]

	Commands:
	  add      Add a new task.
	  modify   Modify/Edit an existing task
	  list     List tasks
	  start    Start task(s)
	  open     Open task(s)
	  pause    Pause task(s)
	  stop     Stop task(s)
	  comment  Set or Get comment for task(s)
	  info     Get all data about selected task(s)
	  switch   Switch workspace
	  import   Import tasks from a specified directory
	  export   Export tasks to a specified directory
	  log      Log drawdown
	  help     Print this message or the help of the given subcommand(s)

	Arguments:
	  [FILTERS]...  Search filters (zero or more)

	Options:
	  -v...                      Increase verbosity (-vvv supported)
	  -e, --endpoint <ENDPOINT>  taud JSON-RPC endpoint [default: tcp://127.0.0.1:23330]
	  -h, --help                 Print help
	  -V, --version              Print version

```shell
% tau [SUBCOMMAND] --help
```

### Quick start

#### Add tasks

Add a new task with the title "review tau usage" with the description text
"description" set to "review tau".

```bash
tau add review tau usage desc:"review tau"
```

Add another task with the title "second task" assigned to dave.
Because no description is set, it will open your EDITOR and prompt you
for a description which allows entering multiline text.

```bash
tau add second task @dave
```

```
% tau add Third task project:tau rank:1.1
% tau add Fourth task @dave project:tau due:1509 rank:2.5
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
% tau @dave			# tasks that assign field is "dave"
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
% tau 3 comment				# will open the editor to write a comment
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

## Local Deployment

### Seed Node

First you must run a seed node. The seed node is a static host which nodes can
connect to when they first connect to the network. The `seed_session` simply
connects to a seed node and runs `protocol_seed`, which requests a list of
addresses from the seed node and disconnects straight after receiving them.

    # P2P accept addresses
    inbound=["127.0.0.1:11001"] 

Note that the above config doesn't specify an external address since the
seed node shouldn't be advertised in the list of connectable nodes. The seed
node does not participate as a normal node in the p2p network. It simply allows
new nodes to discover other nodes in the network during the bootstrapping phase.

### Inbound Node

This is a node accepting inbound connections on the network but which is not
making any outbound connections.

The external addresses are important and must be correct.

    # P2P accept addresses
    inbound=["127.0.0.1:11002"]
    
    # P2P external addresses
    external_addr=["127.0.0.1:11002"]

    # Seed nodes to connect to 
    seeds=["127.0.0.1:11001"]

### Outbound Node

This is a node which has 8 outbound connection slots and no inbound connections.
This means the node has 8 slots which will actively search for unique nodes to
connect to in the p2p network.

    # Connection slots
    outbound_connections=8

    # Seed nodes to connect to 

