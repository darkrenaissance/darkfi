# P2P Network

We instantiate a `p2p` network and call `start()`. This will begin running a single
p2p network until `stop()` is called.

There are 3 session types:

* `InboundSession`, concerned with incoming connections
* `OutboundSession`, concerned with outgoing connections
* `SeedSession` is a special session type which connects to seed nodes to populate
  the hosts pool, then finishes once synced.

Connections are made by either `Acceptor` or `Connector` for incoming or outgoing
respectively. They have multiple transport types; see `src/net/transport/` for the
full list.

Connections are then wrapped in a `Channel` abstraction which allows
protocols to be attached. See `src/net/protocol/` and run `fd protocol` for custom
application specific network protocols. Also see the follow tutorial:

* [Understanding Protocols](learn/dchat/creating-dchat/protocols.md)

## Outbound Session

The outbound session is responsible to ensure the hosts pool is populated, either
through currently connected nodes or using the seed session.
It performs this algorithm:

1. Start $N$ slots, and a sleeping peer discovery process

Then each slot performs this algorithm:

1. If no addresses matching our filters are in the hosts pool then:
    1. Wakeup the peer discovery process. This does nothing if peer discovery is already active.
    2. Peer discovery tries first 2 times to poll the current network if there are connected nodes,
       otherwise it will do a seed server sync.
    3. Peer discovery then wakes any sleeping slots and goes back to sleep.

## Security

* **Backoff/falloff**. This is the strategy implemented in Bitcoin. This can be bad when arbitrary limits are implemented
  since we slow down traffic for no reason.
* **Choking controller**. BitTorrent no longer uses naive tit-for-tat, instead libtorrent implements an anti-leech seeding algo
  from the paper [Improving BitTorrent: A Simple Approach](https://qed.usc.edu/papers/ChowGM08.pdf), which is focused on distributing
  bandwidth to all peers. See also [libtorrent/src/choker.cpp](https://github.com/arvidn/libtorrent/blob/RC_2_0/src/choker.cpp).
    * All p2p messages will have a score which represents workload for the node. There is a hard limit, and in general the choker
      will try to balance the scores between all available channels.
* **Smart ban**. Malicious peers which violate protocols are hard banned. For example sending the wrong data for a chunk.
    * Add a method `channel.ban()` which immediately disconnects and blacklists the address.
* **uTP congestion control**. BitTorrent implements a UDP protocol with its own congestion control. We could do such a similar strategy
  with the addition of removing ordering. This reduces protocol latency mitigating attacks. See [libtorrent.org/utp.html](https://libtorrent.org/utp.html)
  for more info.
    * Maybe less important if we use alternative networks like Tor or i2p.
* **White, gray and black lists**. See section 2.2 of [Exploring the Monero P2P Network](https://eprint.iacr.org/2019/411.pdf) for
  details of this algorithm. This aids with network connectivity, avoiding netsplits which could make the network more susceptible to
  eclipse/sybil attacks (large scale MiTM).
    * For this we would need a function to connect to a host, send a ping, receive a pong and disconnect to test node connectivity.

