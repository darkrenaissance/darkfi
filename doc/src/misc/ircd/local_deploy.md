# Ircd Local Deployment

These steps below are only for developers who wish to make a testing
deployment. The previous sections are sufficient to join the chat.

## Seed Node

First you must run a seed node. The seed node is a static host which
nodes can connect to when they first connect to the network. The
`seed_session` simply connects to a seed node and runs `protocol_seed`,
which requests a list of addresses from the seed node and disconnects
straight after receiving them.

The first time you run the program, a config file will be created in
`~/.config/darkfi` if you are using Linux or in 
`~/Library/Application Support/darkfi/` on MacOS. 
You must specify an inbound accept address in your config file to configure a seed node:

```toml
## P2P accept addresses
inbound=["127.0.0.1:11001"]
```

Note that the above config doesn't specify an external address since
the seed node shouldn't be advertised in the list of connectable
nodes. The seed node does not participate as a normal node in the
p2p network. It simply allows new nodes to discover other nodes in
the network during the bootstrapping phase.

## Inbound Node

This is a node accepting inbound connections on the network but which
is not making any outbound connections.

The external addresses are important and must be correct.

To run an inbound node, your config file must contain the following
info:
		
```toml
## P2P accept addresses
inbound=["127.0.0.1:11002"]

## P2P external addresses
external_addr=["127.0.0.1:11002"]

## Seed nodes to connect to 
seeds=["127.0.0.1:11001"]
```
## Outbound Node

This is a node which has 8 outbound connection slots and no inbound
connections.  This means the node has 8 slots which will actively
search for unique nodes to connect to in the p2p network.

In your config file:

```toml
## Connection slots
outbound_connections=8

## Seed nodes to connect to 
seeds=["127.0.0.1:11001"]
```

## Attaching the IRC Frontend

Assuming you have run the above 3 commands to create a small model
testnet, and both inbound and outbound nodes above are connected,
you can test them out using weechat.

To create separate weechat instances, use the `--dir` command:

    weechat --dir /tmp/a/
    weechat --dir /tmp/b/

Then in both clients, you must set the option to connect to temporary
servers:

    /set irc.look.temporary_servers on

Finally you can attach to the local ircd instances:

    /connect localhost/6667
    /connect localhost/6668

And send messages to yourself.

