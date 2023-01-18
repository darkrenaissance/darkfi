# Dnetview

A simple tui to explore darkfi ircd network topology.

dnetview displays: 

1. all active nodes
2. outgoing, incoming and manual sessions
3. each associated connection and recent messages.

## Install 

```shell
% git clone https://github.com/darkrenaissance/darkfi 
% cd darkfi
% make BINS=dnetview
```

## Usage

Run dnetview as follows:

```shell
dnetview -v
```

On first run, dnetview will create a config file in .config/darkfi. You
must manually enter the RPC ports of the nodes you want to connect to
and title them as you see fit.

Dnetview creates a logging file in /tmp/dnetview.log. To see json data
and other debug info, tail the file like so:

```shell
tail -f /tmp/dnetview.log
```

