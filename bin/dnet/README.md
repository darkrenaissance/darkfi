# dnet

A simple tui to explore darkfi p2p network topology. Displays:

1. Active p2p nodes
2. Outgoing, incoming, manual and seed sessions
3. Each associated connection and recent messages.

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

To use `dnet` you will need to open the config file and modify it. Enter
the RPC ports of the nodes you want to connect to and title them as you
see fit. The default config file uses localhost, but you can replace
this with hostnames or external IP addresses. You must also specify
whether it is a `NORMAL` or a `LILITH` node.

## Usage

Navigate up and down using the arrow keys. Scroll the message log using
`PageUp` and `PageDown`. Type `q` to quit.

## Logging

dnet creates a log file in `bin/dnet/dnet.log`. To see json data and
other debug info, tail the file like so:

```shell
tail -f dnet.log
```
