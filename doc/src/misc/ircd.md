# P2P IRC

In DarkFi, we organize our communication using resilient and
censorship-resistant infrastructure. For chatting, `ircd` is a
peer-to-peer implementation of an IRC server in which any user can
participate anonymously using any IRC frontend and by running the
IRC daemon. `ircd` uses the DarkFi P2P engine to synchronize chats
between hosts.


## Installation

```shell
% git clone https://github.com/darkrenaissance/darkfi 
% cd darkfi
% make BINS=ircd
% sudo make install BINS=ircd
```

## Usage (DarkFi Network)

Upon installing `ircd` as described above, the preconfigured defaults
will allow you to connect to the network and start chatting with the
rest of the DarkFi community.

First, try to start `ircd` from your command-line so it can spawn its
configuration file in place. The preconfigured defaults will autojoin
you to the `#dev` channel, where the community is most active and
talks about DarkFi development.

```shell
% ircd
```

After running it for the first time, `ircd` will create a configuration
file you can review and potentially edit. It might be useful if you
want to add other channels you want to autojoin (like `#philosophy`
and `#memes`), or if you want to set a shared secret for some channel
in order for it to be encrypted between its participants.

When done, you can run `ircd` for the second time in order for it to
connect to the network and start participating in the P2P protocol:

```shell
% ircd
```

## Clients

### Weechat

In this section, we'll briefly cover how to use the [Weechat IRC
client](https://github.com/weechat/weechat) to connect and chat with
`ircd`.

Normally, you should be able to install weechat using your
distribution's package manager. If not, have a look at the weechat
[git repository](https://github.com/weechat/weechat) for instructions
on how to install it on your computer.

Once installed, we can configure a new server which will represent our
`ircd` instance. First, start weechat, and in its window - run the
following commands (there is an assumption that `irc_listen` in the
`ircd` config file is set to `127.0.0.1:11066`):

```
/server add darkfi localhost/11066 -autoconnect
/save
/quit
```

This will set up the server, save the settings, and exit weechat.
We can now proceed with installing the `mallumo` weechat script, which
is used for E2E encryption in private messages on this IRC network.

#### E2E encryption with mallumo

`mallumo` is a Python plugin for Weechat that can be used to
enable end-to-end encryption for private messages between you and
other users of the DarkFi IRC network. The verbose installation
and configuration instructions can be found in the [mallumo git
repository](https://github.com/darkrenaissance/mallumo).

Briefly, make sure you install python3 and
[`pynacl`](https://github.com/pyca/pynacl/) (can usually be done with
your distribution's package manager or `pip`).

Then find where weechat has put its configuration files. It is usually
`~/.weechat` or `~/.local/share/weechat` (here we will assume the
latter). Go to the directory, clone the repo, and make a couple of
symlinks:

```shell
% cd ~/.local/share/weechat
% mkdir -p src
% git clone https://github.com/darkrenaissance/mallumo src/mallumo
% cd python
% ln -s `realpath ../src/mallumo/mallumo` mallumo
% ln -s `realpath ../src/mallumo/mallumo/__init__.py` autoload/mallumo.py
```

Refer to
[darkrenaissance/mallumo](https://github.com/darkrenaissance/mallumo)
and its README for usage instructions.

After this has been set up, the next time you start `ircd` and then
`weechat`, you will be connected to the DarkFi IRC network and be
able to chat with other participants.


## Usage (Local Deployment)

### Seed Node

First you must run a seed node. The seed node is a static host which
nodes can connect to when they first connect to the network. The
`seed_session` simply connects to a seed node and runs `protocol_seed`,
which requests a list of addresses from the seed node and disconnects
straight after receiving them.

The first time you run the program, a config file will be created in
.config/darkfi. You must specify an inbound accept address in your
config file to configure a seed node:

```toml
## P2P accept address
inbound="127.0.0.1:11001" 
```

Note that the above config doesn't specify an external address since
the seed node shouldn't be advertised in the list of connectable
nodes. The seed node does not participate as a normal node in the
p2p network. It simply allows new nodes to discover other nodes in
the network during the bootstrapping phase.

### Inbound Node

This is a node accepting inbound connections on the network but which
is not making any outbound connections.

The external address is important and must be correct.

To run an inbound node, your config file must contain the following
info:
		
```toml
## P2P accept address
inbound="127.0.0.1:11002" 

## P2P external address
external_addr="127.0.0.1:11002"

## Seed nodes to connect to 
seeds=["127.0.0.1:11001"]
```
### Outbound Node

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

### Attaching the IRC Frontend

Assuming you have run the above 3 commands to create a small model
testnet, and both inbound and outbound nodes above are connected,
you can test them out using weechat.

To create separate weechat instances, use the `--dir` command:

    weechat --dir /tmp/a/
    weechat --dir /tmp/b/

Then in both clients, you must set the option to connect to temporary
servers:

    /set irc.look.temporary_servers on

Finally you can attach to the local IRCd instances:

    /connect localhost/6667
    /connect localhost/6668

And send messages to yourself.

### Running a Fullnode

See the script `script/run_node.sh` for an example of how to deploy
a full node which does seed session synchronization, and accepts both
inbound and outbound connections.
