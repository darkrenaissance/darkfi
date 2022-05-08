# Tau

Encrypted tasks management app using peer-to-peer network and raft consensus.  
multiple users can collaborate by working on the same tasks, and all users will have synced task list.


## Install 

```shell
% git clone https://github.com/darkrenaissance/darkfi 
% cd darkfi
% make BINS="taud tau"
% make install "BINS=taud tau" PREFIX=/home/${USER}/.local
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


Also note that for the first time ever running seed node you must run it with 
`--key-gen`:
```shell
% taud --key-gen
```
This will generate a new secret key in `/home/${USER}/.config/tau/secret_key` that 
you can share with nodes you want them to get and decrypt your tasks, otherwise if you
have already generated or got a copy from a peer place it in the same directory
`/home/${USER}/.config/tau/secret_key`.


## Usage (CLI)

```shell
% tau --help 
```
	tau 0.3.0
	Tau cli
	
	USAGE:
	    tau [FLAGS] [OPTIONS] [ARGS] [SUBCOMMAND]
	
	FLAGS:
	    -h, --help       Prints help information
	    -V, --version    Prints version information
	    -v               Increase verbosity
	
	OPTIONS:
	    -c, --config <config>     Sets a custom config file
	        --rpc <rpc-listen>    JSON-RPC listen URL [default: 127.0.0.1:11055]
	
	ARGS:
	    <id>            Get task by ID
	    <filters>...    Search criteria (zero or more)
	
	SUBCOMMANDS:
	    add        Add a new task
	    comment    Set or Get comment for a task
	    help       Prints this message or the help of the given subcommand(s)
	    list       List all tasks
	    state      Set or Get task state
	    update     Update/Edit an existing task by ID

```shell
% tau help [SUBCOMMAND]
```

### Example  

```shell
$ # add new task  
$ tau add "new title"   
$ tau add "new title" project:blockchain desc:"new description" rank:3 assign:dark
$
$ # lists tasks
$ tau  		   		 
$ tau open 			 # open tasks
$ tau pause 		 # paused tasks
$ tau 0522 		 	 # created at May 2022
$ tau project:blockchain assign:dark
$ tau rank:gt:n  # lists all tasks that have rank greater than n
$ tau rank:ls:n  # lists all tasks that have rank lesser than n
$
$ # update task 
$ tau update 3 project:network  rank:20
$
$ # state 
$ tau state 3  # get state
$ tau state 3 pause  # set the state to pause 
$
$ # comments 
$ tau comments 1  # list comments
$ tau comments 3 "new comment"  # add new comment 
```



