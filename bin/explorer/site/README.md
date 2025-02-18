Blockchain explorer web front-end
=======

This is a very basic python based web front-end, based on `flask`,
to serve static pages with blockchain data.

## Usage

We fist have to run 2 other daemons, to retrieve data from.
Note: all paths are from repo root.

First we start a `darkfid` localnet:

```
% cd contrib/localnet/darkfid-single-node/
% ./tmux_sessions.sh
```

It is advised to shutdown the `minerd` daemon after couple of blocks, to not waste resources.

Update the `explorerd` configuration to the localnet `darkfid` JSON-RPC endpoint
and start the daemon:

```
% cd bin/explorer/explorerd
% make install
% explorerd -c explored_config.toml  
```

Then we enter the site folder and we generate a new python virtual environment,
source it and install required dependencies:

```
% cd bin/explorer/site
% python -m venv venv
% source venv/bin/activate
% pip install -r requirements.txt
```

To start the `flask` server, simply execute:

```
% python -m flask run
```

The web site will be available at `127.0.0.1:5000`.
