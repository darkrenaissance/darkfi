# deg (Debugging Event Graph)

A simple tui to explore darkfi's Event Graph state. Displays:

1. Active components of EventGraph.
2. Live protocol msgs.
3. Update active components.

## Run

### Using a venv

`Deg` requires Python 3.12.0. Make sure Python is installed and on the
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

Run deg:

```shell
% ./deg
```

You will need to reactivate the venv in your current terminal session
each time you use `deg` as follows:

```shell
% source python-env/bin/activate
```

### Without a venv

If you don't require a venv, install the requirements and run `deg` as follows:

```shell
% pip install -r requirements.txt
% python main.py
```

## Config

On first run, `deg` will create a config file in the config directory
specific to your operating system.

To use `deg` you will need to open the config file and modify it. Enter
the RPC ports of the nodes you want to connect to and title them as you
see fit. The default config file uses localhost, but you can replace
this with hostnames or external IP addresses. You must also specify
whether it is a `NORMAL` or a `LILITH` node.

## Usage

Navigate up and down using the arrow keys. Scroll the message log using
`PageUp` and `PageDown`. Type `q` to quit.

## Logging

deg creates a log file in `bin/deg/deg.log`. To see json data and
other debug info, tail the file like so:

```shell
tail -f deg.log
```
