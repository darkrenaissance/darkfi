# dnet

A simple tui to explore darkfi p2p network topology. Connects to nodes
on the darkfi network using RPC and displays the following info:

1. Every inbound, outbound and manual connections.
2. Events such as peer discovery, new connections, disconnections etc.
3. All messages per connection.

All darkfi node types are supported, i.e. darkfid, darkirc, taud,
fud, etc.

`dnet` is based on the design-pattern Model, View, Controller. We create
a logical seperation between the underlying data structure or Model;
the ui rendering aspect which is the View; and the Controller or game
engine that makes everything run.

## Run

### Using a venv

Dnet requires Python 3.12.0. Make sure Python is installed and on the
latest version.

Depending on your setup you may need to install a virtual environment
for Python. Do so as follows:

```shell
% python -m venv python-env
% source python-env/bin/activate
```

Then install the requirements:

```shell
% pip install -r requirements.txt
```

Run dnet:

```shell
% python dnet
```

You will need to reactivate the venv in your current terminal session
each time you use `dnet` as follows:

```shell
% source python-env/bin/activate
```

### Without a venv

If you don't require a venv, install the requirements and run dnet as follows:

```shell
% pip install -r requirements.txt
% python dnet
```

## Config

On first run, `dnet` will create a config file in the config directory
specific to your operating system.

To use `dnet` you will need to open the config file and modify it to
display the individual nodes you want to inspect. By node we mean daemon
such as darkirc, darkfid, taud etc. Each node in the `dnet` config has
the following parameters:

* `name`: An arbitary string (whatever you want to call your node,
e.g. darkirc).
* `host`: The network host, set to `localhost` by default, but you can
replace this with hostnames or external IP addresses.
* `port`: The `rpc_listen` port of the node you want to connect to.
* `type`: Specify whether it is a `NORMAL` or a `LILITH` node. (If you
don't know what this means, such stick with `NORMAL`).

Next, make sure that this line is commented in the config file of the
node you are trying to connect to:

```toml
## Disabled RPC methods
#rpc_disabled_methods = ["p2p.get_info"]
```

If necessary, restart the node to apply the change.

## Usage

Navigate up and down using the arrow keys. Scroll the message log using
`PageUp` and `PageDown`. Type `q` to quit.

## Logging

dnet creates a log file in `bin/dnet/dnet.log`. To see json data and
other debug info, tail the file like so:

```shell
tail -f dnet.log
```
