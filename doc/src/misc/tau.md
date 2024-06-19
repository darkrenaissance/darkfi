# Tau

Encrypted tasks management app using peer-to-peer network.
Multiple users can collaborate by working on the same tasks, 
and all users will have synced tasks.


## Install 

```shell
% git clone https://codeberg.org/darkrenaissance/darkfi
% cd darkfi
% make BINS="taud"
```

If you want to have `taud` system wide:
```shell
% sudo make install BINS="taud"
```

And then run the daemon:
```shell
% taud
```

For the CLI part of `tau`, we use `tau-python`, you can run it with:
```shell
% cd bin/tau/tau-python
% ./tau
```
Or you can alias it by adding this line to your `bashrc`:
```shell
% alias tau=/path-to-darkfi/bin/tau/tau-python/tau
```

## Usage 

To run your own instance check [Local Deployment](#local-deployment)

```shell
% tau --help 
```
	USAGE:
		tau [OPTIONS] [SUBCOMMAND]

	OPTIONS:
		-h, --help                   Print help information

	SUBCOMMANDS:
		add        Add a new task.
		archive    Show completed tasks.
		comment    Write comment for task by id.
		modify     Modify an existing task by id.
		pause      Pause task(s).
		start      Start task(s).
		stop       Stop task(s).
		switch     Switch between configured workspaces.
		show       List filtered tasks.
		help       Show this help text.


### Quick start

#### Add tasks

Add a new task with the title "review tau usage" with the description text
"description" set to "review tau".

```bash
% tau add review tau usage desc:"review tau"
```

Add another task with the title "second task" assigned to dave.
Because no description is set, it will open your EDITOR and prompt you
for a description which allows entering multiline text.

```bash
% tau add second task @dave
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
% tau show state:open	# list open tasks
% tau rank:2			# all tasks that have rank 2
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
```

#### Comments

```shell
% tau 1 comment "content foo bar"	# will add a comment to task 1
% tau 3 comment				# will open the editor to write a comment
```

#### Export and Import

```shell
% tau export ~/example_dir	# will save tasks json files to the path
% tau import ~/example_dir	# will reload saved json files from the path
```

### archive

```shell
% tau archive                 # current month's completed tasks
% tau archive 1122            # completed tasks in Nov. 2022
% tau archive 1 1122          # show info of task by it's ID completed in Nov. 2022
```

#### Switch workspace

```shell
% tau switch darkfi	# darkfi workspace needs to be configured in config file
```

In addition to indexing tasks by there IDs, one can use their RefID (Reference ID):
```shell
% tau SjJ2OANxVIdLivItcrMplpOFbLWgzR
# or
% tau SjJ2OANxV
# or even
% tau SjJ
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

