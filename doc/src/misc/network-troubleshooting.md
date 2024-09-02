# Network troubleshooting

If you're having network issues, refer to this page to debug various 
issues. If you see inconsistencies in the docs: always trust 
`bin/darkirc/darkirc_config.toml` or whichever respective apps' repo
config file. Documentation updates are a current WIP.

The default location for config files is `~/.config/darkfi`

Note: throughout this page we generally assume you are using
`darkirc` since it's our main p2p app currently. If you're
using a different app such as `darkfid` or `taud`, the same
instructions will follow but the app name will change (for example
the config file `.config/darkfi/darkirc_config.toml` will become
`.config/darkfi/taud_config.toml`)

## Common net problems 

The most common problem in connecting to `darkirc` is the following:

```
[ERROR] [EVENTGRAPH] Sync: Could not find any DAG tips
[ERROR] Failed syncing DAG. Exiting.
Error: DagSyncFailed
```

This generally indicates that we were *unable to establish any P2P
connections*, and thus couldn't retrieve the message history required to
sync our messages locally within the time limit (DAG sync failed).

There are two main reasons why we would fail to establish a P2P connection:

1. Seed node is down or rejecting our connection
2. Our node does not have sufficient peers

If the seed node is down, you will see this message in the debug output:

```
ERROR] [P2P] Network reseed failed!
[WARN] [P2P] Unable to connect to seed [tcp+tls://lilith1.dark.fi:5262/]: IO error: connection refused
```

If it's a problem related to nodes, you will typically see a successful
seed connection like so:

```
[INFO] [P2P] Connected seed [tcp+tls://lilith1.dark.fi:5262/]
[INFO] [P2P] Disconnecting from seed [tcp+tls://lilith1.dark.fi:5262/]
```

Followed by multiple connection failed messages, like so:

```
[INFO] [P2P] Unable to connect outbound slot #5 [tcp+tls://dasman.xyz:26661/]: IO error: connection refused
[INFO] [P2P] Unable to connect outbound slot #6 [tcp+tls://[2a02:aa13:8342:1400:8972:cc07:ae3:8676]:26661/]: IO error: host unreachable
```

### Seed node is down

If you get an error like this:

```
[WARN] [P2P] Unable to connect to seed [tcp+tls://lilith1.dark.fi:5262/]: IO error: connection refused
```

This means you are failing to establish a connection to the seed node.

Please note: the IO error might not always read `connection refused`
but could be some other error such as `host unreachable`. Please note
this IO error as it is useful debugging info.

Here's what to do next:

#### It's my first time connecting to the network

If it's your first time connecting to the network, you local node does
not have a record of other peers it can connect to in case the seed node
is down. Please do the following:

1. Take careful note of the `IO error` that is written after `Unable to
connect to seed`.
2. Send a message in the community Telegram (t.me/darkfichat) with the
seed IO error. Please don't send screenshots- instead use pastebin (or
termbin or pastenym) for multi-line errors, or just copy paste for a
single line error.
3. You can set a peer such as `tcp+tls://example_peer:26661/` in your
config file. Ask in the telegram community channel for an active peer
(here we are using a fake peer called `example_peer`. Then open the
config file at `.local/config/darkirc_config.toml` and modify the `peers`
field with the provided peer as follows:

```
peers =  tcp+tls://example_peer:26661/
```

#### It's not my first time connecting to the network

If it's not your first time connecting to the network, you should be
able to establish connections to peers even if the seed node is down.

This is possible via a list of hosts that your darkirc node keeps locally.
You can inspect the hostlist as follows:

```
cat .local/darkfi/darkirc/hostlist.tsv
```

If the list is empty, open `.config/darkfi/darkirc_config` and ensure
that the `hostlist` field is set with a path of your chosing.

For example:

```
hostlist = "~/.local/darkfi/darkirc/hostlist.tsv"
```

Then follow the steps in the above section `It's my first time connecting
to the network`.

If the hostlist is not empty, retry the darkirc connection and carefully
note the connection errors that are happening from peers. Report these
errors on the community telegram (t.me/darkfichat).

Please don't send screenshots- instead use pastebin (or termbin or
pastenym) for multi-line errors, or just copy paste for a single line
error.

It might be simply the case that there are not enough peers on the
network, or perhaps there is another issue we are not aware of.

You can also check the liveness of peers using the `ping` tool.

To access the `ping` tool, in the `~/darkfi/script/ping` directory run
`cargo run main.rs`. Once completed, you can now use the `ping` tool in
the `~/darkfi/script/ping/target/debug` directory.

Ping the peers located in your hostlist

```
$ ./ping tcp+tls://example_peer:26661
```
If the peers are reachable, you'll receive a `Connected!` output

### Cannot establish peer connections

If you're able to connect to the seed but are failing to establish peer
connections, please retry the darkirc connection and carefully note the
connection errors that are happening from peers. Report these errors on
the community telegram (t.me/darkfichat). Please don't send screenshots-
instead use pastebin (or termbin or pastenym) for multi-line errors,
or just copy paste for a single line error.

## dnet

dnet is a simple tui to explore darkfi p2p network topology. You can use 
dnet to gather more network information. dnet displays:

1. Active p2p nodes
2. Outgoing, incoming, manual and seed sessions
3. Each associated connection and recent messages.

To install dnet, go [here](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/bin/dnet/README.md#run).
You can use dnet to view the network topology and see how your node 
interacts within the network. dnet log information is created in `bin/dnet/dnet.log`

## Ping tool

You can ping any node to make sure it's online by using the provided
`ping` tool.

To access the `ping` tool, in the `~/darkfi/script/ping` directory run
`cargo run main.rs`. Once completed, you can now use the `ping` tool in
the `~/darkfi/script/ping/target/debug` directory.

Ping the peers located in your hostlist

```
$ ./ping tcp+tls://example_peer:26661
```
If the peers are reachable, you'll receive a `Connected!` output

## Inbound

To see if your address is reachable to others in the network, you'll need 
to use separate device to `ping` your external address. 
[Generate an external address here](https://darkrenaissance.github.io/darkfi/clients/tor_inbound.html?highlight=tor#1-install-tor).
For example purposes, let's assume your external address is 
`jamie3vkiwibfiwucd6vxijskbhpjdyajmzeor4mc4i7yopvpo4p7cyd.onion`. 
`ping` your generated external address from a separate device. 
```
$ ./ping jamie3vkiwibfiwucd6vxijskbhpjdyajmzeor4mc4i7yopvpo4p7cyd.onion
```
If your external address is reachable, you'll receive a `Connected!` prompt

## Check tor connection

You can verify if your local node is running over Tor. Execute this 
command in `~/darkfi/script`. You'll need to install pysocks 
`pip install pysocks` prior to running `tor-test.py` the first time.
```
$ python3 tor-test.py 
```
If your local node is running Tor, the response should be an IP address.
An error will return if Tor isn't running.

### Helpful debug information

If you're looking to debug an issue, try these helpful tools

## Logs in debug mode

You can run any app in debug mode as follows:

`./darkirc -vv`

Alternatively, modify the config file at `.config/darkfi/darkirc.toml' as follows:

```toml
# Log to file. Off by default.
log = "/tmp/darkirc.log"
# Set log level. 1 is info (default), 2 is debug, 3 is trace
verbose = 2
```
## Config file

Your config files are generated in your `~/.config/darkirc` directory. 
You'll have to run each daemon once for the app to spawn a config file, 
which you can review and edit. There is also helpful information within 
the config files.

## Node information script

If you're looking for information about your node, including inbound, 
outbound, and seed connections, execute this command in `~/darkfi/script`
```
$ python3 node_get-info.py
```

## Hostlist issues

If you receive DAG sync issues, verify:

1. a hostlist is set in the config file of the respective app.
2. There are hosts in the hostlists (you should get hostlists from the 
default seed on the first run). You can find the hostlist files within 
the respective apps' repo. For example darkirc's default hostlist location 
is `~/.local/darkfi/darkirc/hostlist.tsv`
