# p2p IRC

This is a local daemon which can be attached to with any IRC frontend.
It uses the darkfi p2p engine to synchronize chats between hosts.


## Install 

```shell
% git clone https://github.com/darkrenaissance/darkfi 
% cd darkfi
% make BINS=ircd
% make install BINS=ircd PREFIX=/home/XX/.local
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

### Attaching the IRC Frontend

Assuming you have run the above 3 commands to create a small model testnet,
and both inbound and outbound nodes above are connected, you can test them
out using weechat.

To create separate weechat instances, use the `--dir` command:

    weechat --dir /tmp/a/
    weechat --dir /tmp/b/

Then in both clients, you must set the option to connect to temporary servers:

    /set irc.look.temporary_servers on

Finally you can attach to the local IRCd instances:

    /connect localhost/6667
    /connect localhost/6668

And send messages to yourself.

### Running a Fullnode

See the script `script/run_node.sh` for an example of how to deploy a full node which
does seed session synchronization, and accepts both inbound and outbound
connections.
