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

If you are using default ports for the daemon you want to debug:
```shell
% # darkirc default RPC is localhost:26660
% ./deg darkirc
% # tau default RPC is localhost:23330
% ./deg tau
```

To use `deg` with non-default host and port you will need to provide 
them from args like so `host:port`:

```shell
% ./deg -e 127.0.0.1:2625
```

## Replay mode

A tool embeded in `deg` to replay someone else's eventgraph.
Since daemons like `darkirc` log the database instructions, one can 
share thier db logs in `/tmp/replayer_log` with us, running in replay
mode we can browse the eventgraph from their point of view and find 
which event is missing or other issues they face.

Running in replay mode is as simple as adding `-r` when running deg:

```shell
% ./deg -r darkirc
% # Or
% ./deg -r -e 127.0.0.1:2625
```

## Usage

Navigate up and down using the arrow keys. Scroll the message log using
`PageUp` and `PageDown`. Press `enter` to view details about selected event.
Press `b` to get back to the main view. Press `q` to quit.
