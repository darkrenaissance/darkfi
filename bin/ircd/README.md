# p2p IRC

This is a local daemon which can be attached to with any IRC frontend.
It uses the darkfi p2p engine to synchronize chats between hosts.

## Local Deployment

### Seed Node

First you must run a seed node. The seed node is a static host which nodes can
connect to when they first connect to the network. The `seed_session` simply
connects to a seed node and runs `protocol_seed`, which requests a list of
addresses from the seed node and disconnects straight after receiving them.

    LOG_TARGETS=net cargo run -- -vv --accept 0.0.0.0:9999 --irc 127.0.0.1:6688

Note that the above command doesn't specify an external address since the
seed node shouldn't be advertised in the list of connectable nodes. The seed
node does not participate as a normal node in the p2p network. It simply allows
new nodes to discover other nodes in the network during the bootstrapping phase.

### Inbound Node

This is a node accepting inbound connections on the network but which is not
making any outbound connections.

The external address is important and must be correct.

    LOG_TARGETS=net cargo run -- -vv --accept 0.0.0.0:11004 --external $LOCAL_IP:11004 --seeds $SEED_IP:9999 --irc 127.0.0.1:6667

### Outbound Node

This is a node which has 8 outbound connection slots and no inbound connections.
This means the node has 8 slots which will actively search for unique nodes to
connect to in the p2p network.

    LOG_TARGETS=net cargo run -- -vv --slots 5 --seeds $SEED_IP:9999 --irc 127.0.0.1:6668

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

See the script `run_node.sh` for an example of how to deploy a full node which
does seed session synchronization, and accepts both inbound and outbound
connections.
