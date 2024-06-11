Anonymous outbound connection
=======================

Using Nym's mixnet to anonymously connect to other peers in `Darkfi` 
network as Nym offers network-level privacy.\
An outbound connection with peers can be done anonymously using Nym, 
we will be proxying our packets through `SOCKS5 Client` to mixnet to
`Network Requester` to other peers and back.\
The following instructions should work on any Linux system.

## 1. **Download**

Nym binaries can be downloaded from [Nym releases](https://github.com/nymtech/nym/releases) 
or pre-built for Ubuntu 20.04 x86-64 from [nymtech website](https://nymtech.net/download-nym-components/).\
Download `SOCKS5 Client` and `Network Requester`.


## 2. **Initialize**

`Network Requester` makes the requests on your behalf, it is 
recommended to run your own on a server, however for the sake of 
example and simplicity everything is run locally.\
We'll start by initializng the `Network Requester`:

```
% ./nym-network-requester init --id nettestnode
```

This will print some information in the terminal, what we want is the 
client address, for example it could be something like this:

```
The address of this client is: 8hUvtEyZK8umsdxxPS2BizQhEDmbNeXEPBZLgscE57Zh.5P2bWn6WybVL8QgoPEUHf6h2zXktmwrWaqaucEBZy7Vb@5vC8spDvw5VDQ8Zvd9fVvBhbUDv9jABR4cXzd4Kh5vz
```

Then we'll use that address as provider for `SOCKS5 Client` 
initialization:

```
% ./nym-socks5-client init --use-reply-surbs true --id sockstest --provider 8hUvtEyZK8umsdxxPS2BizQhEDmbNeXEPBZLgscE57Zh.5P2bWn6WybVL8QgoPEUHf6h2zXktmwrWaqaucEBZy7Vb@5vC8spDvw5VDQ8Zvd9fVvBhbUDv9jABR4cXzd4Kh5vz
```

We also set `--use-reply-surbs` flag to true, this will enable 
anonymous sender tag for communication with the service provider, 
but it will make the actual communication slower.

## 3. **Run**

Now we can run `Network Requester` and then `SOCKS5 Client`:

```
% ./nym-network-requester run --id nettestnode
```

Then in another terminal run:

```
% ./nym-socks5-client run --id sockstest
```

> Adding a new domain/address to `allowed.list` while 
`nym-network-requester` is running you must restart it to pick up the 
new list.

Both of these binaries have to be running when setting up a node.

Currently connecting to other nodes might not be as dynamic as you'd 
think, there are two things we can do here:

**1. `Network Requester` as open proxy:**

you only need to run it like:

```
% ./nym-network-requester run --id nettestnode --open-proxy
```

This makes the whitelist not needed anymore, meaning you don't need to 
worry about adding peers to `allowed.list` anymore, but don't share
the address of the `Network Requester` while running as open proxy
randomly.

**2. whitelisted addresses approach, here's how it works:**

- Initialize `nym-network-requester`
- Initialize `nym-socks5-client`
- Add known peers' domains/addresses to `~/.nym/service-providers/network-requester/allowed.list`
- Run `nym-network-requester`
- Run `nym-socks5-client`
- Edit Darkfi node's config file (provided in the next section) so you 
can connect to peers manually, or through seed.

> Note that for peer discovery you'll have to whitelist some known 
peers and the seed itself.


## 4. **Setup `darkirc`**

After compiling `darkirc`, run it once to spawn the config file. Then
edit it to contain the following:

```toml
# manually
## P2P net settings
[net]
outbound_connections=0
peers = ["nym://some.whitelisted.domain:25552", "nym://someother.whitelisted.domain:25556"]
outbound_transports = ["nym"]

# automatically
## P2P net settings
[net]
outbound_connections=8
seeds = ["nym://some.whitelisted.seed:25551", "tcp://someother.whitelisted.seed:25551"]
outbound_transports = ["nym"]
```

> The most important part that could easily be forgotten is: ```outbound_transports = ["nym"]```

Now when you start `darkirc`, you will be able to discover or connect 
directly to peers and your traffic will be routed through the mixnet.

These instructions are also applicable to other nodes in the DarkFi
ecosystem, e.g. `darkfid`.
