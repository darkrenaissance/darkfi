# deg (Debugging Event Graph)

A simple tui to explore darkfi's Event Graph state. Displays:

1. List of current events in the DAG.
2. Minimal graph showing how events are linked.
3. A view for more details.

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
% ./deg
```

## Config

On first run, `deg` will create a config file in the config directory
specific to your operating system.

To use `deg` you will need to open the config file and modify it. Enter
the RPC port of the node you want to connect to and title them as you
see fit. The default config file uses localhost, but you can replace
this with hostname or external IP address.

If you are using default ports for the daemon you want to debug:
```shell
% # darkirc default RPC is localhost:26660
% ./deg darkirc
% # tau default RPC is localhost:23330
% ./deg tau
```

## Usage

Navigate up and down using the arrow keys. Scroll the message log using
`PageUp` and `PageDown`. Press `enter` to view details about selected event.
Type `b` to get back to the main view. Type `q` to quit.
