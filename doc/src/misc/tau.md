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
	    tau [OPTIONS] [SUBCOMMAND]
	
	OPTIONS:
	    -h, --help               Print help information
	        --listen <LISTEN>    Rpc address [default: 127.0.0.1:8875]
	    -v                       Increase verbosity
	    -V, --version            Print version information
	
	SUBCOMMANDS:
	    add            Add a new task
	    get            Get task by ID
	    get-comment    Get task's comments
	    get-state      Get task state
	    help           Print this message or the help of the given subcommand(s)
	    list           List open tasks
	    set-comment    Set comment for a task
	    set-state      Set task state
	    update         Update/Edit an existing task by ID


