# Network troubleshooting

If you're having network issues, refer to this page to debug various issues. If you see inconsistencies in the docs: always trust bin/darkirc/darkirc_config.toml or whichever respective apps' repo config file. Documentation updates are a current WIP.

The default location for config files is `~/.config/darkfi`

## Check liveness of seed nodes

Use the `ping` tool to check if your node can access the seeds on the network. To access the `ping` tool, in the `~/darkfi/script/ping` directory run `cargo run main.rs`. Once completed, you can now use the `ping` tool in the `~/darkfi/script/ping/target/debug` directory. 

Ping tcp seeds located in your config file
```
$ ./ping tcp://lilith0.dark.fi:5262
$ ./ping tcp://lilith1.dark.fi:5262
```
If the tcp seeds are reachable, you'll receive a `Connected!` output

ping tcp+tls seeds located in your config file
```
$ ./ping tcp+tls://lilith0.dark.fi:5262
$ ./ping tcp+tls://lilith1.dark.fi:5262
```
If the tcp+tls seeds are reachable, you'll receive a `Connected!` output

If these work, then your node is connected to seeds on the network.

## dnet

dnet is a simple tui to explore darkfi p2p network topology. You can use dnet to gather more network information. dnet displays:
1. Active p2p nodes
2. Outgoing, incoming, manual and seed sessions
3. Each associated connection and recent messages.

To install dnet, go [here](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/bin/dnet/README.md#run).
You can use dnet to view the network topology and see how your node interacts within the network. dnet log information is created in `bin/dnet/dnet.log`


## Inbound

To see if your address is reachable to others in the network, you'll need to use separate device to `ping` your external address. 
[Generate an external address here](https://darkrenaissance.github.io/darkfi/clients/tor_inbound.html?highlight=tor#1-install-tor).
For example purposes, let's assume your external address is `jamie3vkiwibfiwucd6vxijskbhpjdyajmzeor4mc4i7yopvpo4p7cyd.onion`. `ping` your generated external address from a separate device. 
```
$ ./ping jamie3vkiwibfiwucd6vxijskbhpjdyajmzeor4mc4i7yopvpo4p7cyd.onion
```
If your external address is reachable, you'll receive a `Connected!` prompt

## Check tor connection
You can verify if your local node is running over Tor. Execute this command in `~/darkfi/script`. You'll need to install pysocks `pip install pysocks` prior to running `tor-test.py` the first time.
```
$ python3 tor-test.py 
```
If your local node is running Tor, the response should be an IP address. An error will return if Tor isn't running.

### Helpful debug information

If you're looking to debug an issue, try these helpful tools

## Logs in debug mode

When looking for log information refer to the respective apps' config file. 
Change the following settings in the configuration file, `~/.config/darkirc/darkirc_config.toml` in this example

```toml
# Log to file. Off by default.
log = "/tmp/darkirc.log"
# Set log level. 1 is info (default), 2 is debug, 3 is trace
verbose = 2
```
## Config file

Your config files are generated in your `~/.config/darkirc` directory. You'll have to run each daemon once for the app to spawn a config file, which you can review and edit. There is also helpful information within the config files.

## Node information script

If you're looking for information about your node, including inbound, outbound, and seed connections, execute this command in `~/darkfi/script`
```
$ python3 node_get-info.py
```

## Hostlist issues

If you receive DAG sync issues, verify:
1. a hostlist is set in the config file of the respective app.
2. There are hosts in the hostlists (you should get hostlists from the default seed on the first run). You can find the hostlist files within the respective apps' repo. For example darkirc's default hostlist location is `~/.local/darkfi/darkirc/hostlist.tsv`

If you are running MacOS, you should [use tor](https://darkrenaissance.github.io/darkfi/clients/tor_inbound.html?highlight=tor#hosting-anonymous-nodes).
