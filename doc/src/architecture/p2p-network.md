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

1. Start $N$ slots, and set each slot with `status = ACTIVE`

Then each slot performs this algorithm:

1. If no addresses matching our filters are in the hosts pool then:
    1. If there is another slot where `status ≟ DISCOVERY` or `status ≟ SEED` then
       let `status = SLEEP` and wait for a wakeup signal.
    2. If there are channels opened in `p2p` then let `status = DISCOVERY`
       else skip this step, and let `status = SEED`.
        1. If `status ≟ DISCOVERY` and no hosts are found then let `status = SEED`.
    3. In either case when `status ≟ DISCOVERY` or `status = SEED` and we manage to find
       new hosts, then wakeup the other sleeping slots.
    4. If there are still no hosts found, then let `status = SLEEP`.

The slots are able to communicate to each other through pipes to signal status changes
such as wakeup requests.

Sleeping slots are woken up periodically by the session. They can be forcefully woken up
by calling `session.wakeup()`.

## Security

* **Backoff/falloff**. This is the strategy implemented in Bitcoin. This can be bad when arbitrary limits are implemented
  since we slow down traffic for no reason.
* **Choking controller**. BitTorrent no longer uses naive tit-for-tat, instead libtorrent implements an anti-leech seeding algo
  from the paper [Improving BitTorrent: A Simple Approach](https://qed.usc.edu/papers/ChowGM08.pdf), which is focused on distributing
  bandwidth to all peers. See also [libtorrent/src/choker.cpp](https://github.com/arvidn/libtorrent/blob/RC_2_0/src/choker.cpp).
* **Smart ban**. Malicious peers which violate protocols are hard banned. For example sending the wrong data for a chunk.
* **uTP congestion control**. BitTorrent implements a UDP protocol with its own congestion control. We could do such a similar strategy
  with the addition of removing ordering. This reduces protocol latency mitigating attacks. See [libtorrent.org/utp.html](https://libtorrent.org/utp.html)
  for more info.
    * Maybe less important if we use alternative networks like Tor or i2p.
* **White, gray and black lists**. See section 2.2 of [Exploring the Monero P2P Network](https://eprint.iacr.org/2019/411.pdf) for
  details of this algorithm. This aids with network connectivity, avoiding netsplits which could make the network more susceptible to
  eclipse/sybil attacks (large scale MiTM).
    * For this we would need a function to connect to a host, send a ping, receive a pong and disconnect to test node connectivity.

