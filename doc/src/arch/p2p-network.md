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

## Hostlist filtering

Node maintain a hostlist consisting of three parts, a whitelist, a
greylist and an anchorlist. Each hostlist entry is a tuple of two parts,
a URL address and a `last_seen` data field, which is a timestamp of the
last time the peer was interacted with.

The lists are ordered chronologically according to `last_seen`, with the
most recently seen peers at the top of the list. The whitelist max size
is 1000. The greylist max size is 5000. If the number of peers in these
lists reach this maximum, then the peers with the oldest `last_seen`
fields are removed from the list.

Each time a node receives info about a set of peers, the info is
inserted into its greylist. To discover peers, nodes broadcast `GetAddr`
messages. Upon receiving a `GetAddr` message, peers reply with an `Addr`
message containing their whitelist. The requester inserts the received
peer data into its greylist.

Nodes update their hostlists through a mechanism called
"greylist housekeeping", which periodically pings randomly selected peers
from its greylist. If a peer is responsive, then it is promoted to the
whitelist with an updated `last_seen` field, otherwise it is removed
from the greylist.

On shutdown, whitelist entries are downgraded to greylist. This forces
all whitelisted entries through the greylist refinery each time a node
is started, further ensuring that whitelisted entries are active.

If a connection is established to a host, that host is promoted to 
anchorlist. If anchorlist or whitelist nodes disconnect or cannot
be connected to, those hosts are downgraded to greylist.

Nodes can configure how many anchorlist connections or what percentage
of whitelist connections they would like to make, and this configuration
influences the connection behavior in `OutboundSession`. If there's
not enough anchorlist entries, the connection loop will select from
the whitelist. If there's not enough whitelist entries in the hostlist,
it will select from the greylist.

This design has been largely informed by the [Monero p2p
algo](https://eprint.iacr.org/2019/411.pdf)

## Security

### Design Considerations

* Mitigate attacks to a reasonable degree. Ensuring complete coverage against attacks is infeasible and likely
  introduces significant latency into protocols.
* Primarily target the p2p network running over anonymity networks like Tor, i2p or Nym.
  This means we cannot rely on node addresses being reliable. Even on the clearnet, attackers can easily obtain
  large numbers of proxy addresses.

The main attacks are:

* **Sybil attack**. A malicious actor tries to subvert the network using sockpuppet nodes. For example
  false signalling using version messages on the p2p network.
* **Eclipse attack**. Targets a single node, through a p2p MitM attack where the malicious actor controls all
  the traffic you see. For example they might send you a payment, then want to doublespend without you
  knowing about it.
* **Denial of Service**. Usually happens when a node is overloaded by too much data being sent.

From [libp2p2 DoS mitigation](
https://docs.libp2p.io/concepts/security/dos-mitigation/): "An attack is
considered viable if it takes fewer resources to execute than the damage
it does. In other words, if the payoff is higher than the investment it
is a viable attack and should be mitigated."

### Common Mitigations

* **Backoff/falloff**. This is the strategy implemented in Bitcoin. This can be bad when arbitrary limits are implemented
  since we slow down traffic for no reason.
* **Choking controller**. BitTorrent no longer uses naive tit-for-tat, instead libtorrent implements an anti-leech seeding algo
  from the paper [Improving BitTorrent: A Simple Approach](https://qed.usc.edu/papers/ChowGM08.pdf), which is focused on distributing
  bandwidth to all peers. See also [libtorrent/src/choker.cpp](https://github.com/arvidn/libtorrent/blob/RC_2_0/src/choker.cpp).
    * All p2p messages will have a score which represents workload for the node. There is a hard limit, and in general the choker
      will try to balance the scores between all available channels.
    * Opening the connection itself has a score with inbound connections assigned more cost than outgoing ones.
* **Smart ban**. Malicious peers which violate protocols are hard banned. For example sending the wrong data for a chunk.
    * See the method `channel.ban()` which immediately disconnects and blacklists the address.
* **uTP congestion control**. BitTorrent implements a UDP protocol with its own congestion control. We could do such a similar strategy
  with the addition of removing ordering. This reduces protocol latency mitigating attacks. See [libtorrent.org/utp.html](https://libtorrent.org/utp.html)
  for more info.
    * Maybe less important if we use alternative networks like Tor or i2p.
* **White, gray and black lists**. See section 2.2 of [Exploring the Monero P2P Network](https://eprint.iacr.org/2019/411.pdf) for
  details of this algorithm. This aids with network connectivity, avoiding netsplits which could make the network more susceptible to
  eclipse/sybil attacks (large scale MiTM).
    * See: [Refine Session](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/session/refine_session.rs)
* **Protocol-level reputation system**. You have a keypair, which accrues more trust from the network. Nodes gossip trust metrics.
    * See [AnonRep: Towards Tracking-Resistant Anonymous Reputation](https://www.usenix.org/system/files/conference/nsdi16/nsdi16-paper-zhai.pdf)
    * Also the discussion in
      [Semaphore RLN, rate limiting nullifier for spam prevention in anonymous p2p setting](https://ethresear.ch/t/semaphore-rln-rate-limiting-nullifier-for-spam-prevention-in-anonymous-p2p-setting/5009)
* **Reduce blast radius**. The p2p subsystem should run on its own dedicated executor, separate from database lookups or other
  system operations.
* **fail2ban**
* **Optimized blockchain database**. Most databases are written for interleaved reads and writes as well as deletion. Blockchains follow
  a different pattern of infrequent writes being mostly append-only, and requiring weaker guarantees.

### Protocol Suggestions

Core protocols should be modeled and analyzed with DoS protections added. Below are suggestions to start the investigation.

* Do not forward orphan txs or blocks.
* Drop all double spend txs.
    * Alternatively require a higher fee if we want to enable replace by fee.
    * Do not forward double spend txs.
* Do not forward the same object (block, transaction .etc) to the same peer twice. Violation results in `channel.ban()`.
* Very low fee txs are rate limited.
* Limit orphan txs.
* Drop large or unusual orphan transactions to limit damage.
* Consider a verification cache to prevent attacks that try to trigger re-verification of stored orphan txs. Also limit the size of the cache.
  See [Fixed vulnerability explanation: Why the signature cache is a DoS protection.](https://bitcointalk.org/index.php?topic=136422.0)
* Perform more expensive checks later in tx validation.
* Nodes will only relay valid transactions with a certain fee amount. More expensive transactions require a higher fee.
* Complex operations such as requesting data can be mitigated by tracking the number of requests from a peer.
    * Attackers may attempt flooding invs for invalid data. The peer responds with get data but that object doesn't exist using
      up precious bandwidth. Limit both the rate of invs to 2/s and size of items to 35.
    * Also loops can cause an issue if triggered by the network. This should be carefully analyzed and flattened if possible,
      otherwise they should be guarded against attack.

### Customizable Policy

Apps should be able to configure:

* Reject hosts, for example based off current overall resource utilization or the host addr.
    * Note: we have a configurable setting called `blacklist` which allows us to reject hosts by addr.
* Accounting abstraction for scoring connections.

## Swarming

TODO: research how this is handled on bittorrent. How do we lookup nodes in the swarm? Does the network maintain routing tables?
Is this done through a DHT like Kademlia?

Swarming means more efficient downloading of data specific to a certain subset. A new p2p instance is spawned with a
clean hosts table. This subnetwork is self contained.

An application is for example DarkIRC where everyday a new event graph is spawned. With swarming, you would connect to nodes
maintaining this particular day's event graph.

The feature allows overlaying multiple different features in a single network such as tau, darkirc and so on. New networks require
nodes to bootstrap, but with swarming, we reduce all these networks to a single bootstrap. The overlay network maintaining the
routing tables is a kind of decentralized lilith which keeps track of all the swarms.

Possibly a post-mainnet feature depending on the scale of architectural changes or new code required in the net submodule.

To faciliate this future upgrade, we have made the peer discovery process a generic trait called `PeerDiscoveryBase`. Currently there is only one imeplementation, `PeerDiscovery`, which implements the peer discovery process in outbound sesssion. In the future `PeerDiscoveryBase` can be implemented to make new forms of peer discovery (i.e. subnets vs overlay peer discovery processes).

## Scoring Subsystem

Connections should maintain a scoring system. Protocols can increment the score.

The score backs off exponentially. If the watermark is crossed then the
connection is dropped.

### Libp2p resource manager

In libp2p, resource usage is constrained by a `Resource Manager` that
defines resource usage limits. The `Resource Manager` checks whether
a given request is within a limit and returns an error if it exceeds
a limit.

Resources are deliminated by _Resource Management Scopes_. Each scope
has a corresponding limit that resources cannot exceed.

Limits are calculated by measuring the following resources: 

* Memory 

* File descriptors

* Connections (Inbound connections have stricter limits than outbound
connections)

* Streams: an object of interaction between nodes (~analogous to
`Channel`). Streams are not metered directly- rather they are constrained
within the protocol and service scope (defined below). Inbound streams
are more tightly controlled than outbound streams.

Resource Management Scopes are hierarchial and downstream resource usage
is aggregated at higher levels.

 ```
System
  +------------> Transient.............+................+
  |                                    .                .
  +------------>  Service------------- . ----------+    .
  |                                    .           |    .
  +------------->  Protocol----------- . ----------+    .
  |                                    .           |    .
  +-------------->* Peer               \/          |    .
                     +------------> Connection     |    .
                     |                             \/   \/
                     +--------------------------->  Stream
```

* System scope: top level scope. Nests all other scopes and defines
global hard limits.
* Transcient scope: scope of resources still being established, e.g. a
connection prior to a handshake.
* Service (analogous to `Session`) scopes. Logical groupings of streams
that implement protocol flow and may additionally consume resources such
as memory.
* Protocol scopes. Faciliates backwards compatiability since nodes can
run multiple protocols (incl. old ones) with constrained resource usage.
* Peer scopes. Sets a total limit on the resource usage of an individual
peer.
* Connection scopes. Constrains resource usage by a single
connection. Starts monitoring when the connection begins and ends when
the connection ends.
* Stream (analogous to `Channel`) scopes. Begins when a stream is created
and ends when the stream is closed.
* User transaction scopes. A generic extension to the resource manager
that can be implemented by a programmer.

There is also:

* Allowlist System scope
* Allowlist Transcient scope 

These are System and Transcient scopes for the `allowlist`, which is a
list of honest peer anagolous to our `goldlist`. Allowlist scopes can
continue to use (and meter) resources while the System scope has already
reached its limit (to protect against ellipse attack).

Limits have a default setting that can be configured. It's also possible
to scale limits with a particular config that allows for scaling to
different machines.

### DarkFi p2p resource manager

The goal is to make something simple that we can extend later if necessary
given how it behaves in the wild. We have simplified the libp2p `Resource
management scopes` into a straightforward hierarchy:

```
            node
              +
           channel
              +
       +------|------+
       +             +
    message       protocol  

```

Resource usage is calculated from `Message` and `Protocol` and stored in
`Channel`. We can sum the total resources by adding the total amount
of resources used by each channel using the `p2p` method `channels()`,
(this is easy since `Channel` has access to `p2p` via a weak ptr).

Resources are arranged in a struct called `AbstractComputer` which
contains resource usage indicators such as: CPU, Memory, hard disk,
bandwidth, etc.

The `ScoringSubsystem` monitors scoring actions such as `send_message`
or `recv_message` (and other actions that make use of resources defined
by the `AbstractComputer`) and increments the resource usage.

There is also a `Controller` that defines limits and decides on what action
to take when a given limit has been breached (such as `channel.ban()`,
`channel.throttle()` (TODO), or `choke()`, `snub()` etc (also TODO). It
is important that the limits set by the `Controller` are configurable and
can be injected in at runtime since `Message` and `Protocol` are dynamic,
user-defined types.

TODO:
* implement `ScoringSubsystem`, `AbstractComputer`, and `Controller`.

