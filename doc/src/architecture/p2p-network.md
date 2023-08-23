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
    1. Check the other slots are all `ACTIVE`, otherwise let `status = SLEEP` and
       wait for a wakeup signal.
    2. If we there are channels opened in `p2p` then let `status = DISCOVERY`
       otherwise let `status = SEED`.
    3. If `status ≟ DISCOVERY` and no hosts are found then let `status = SEED`.
    5. In either case when `status ≟ DISCOVERY` or `status = SEED` and we manage to find
       new hosts, then wakeup the other sleeping slots.
    4. If there are still no hosts found, then let `status = SLEEP`.

The slots are able to communicate to each other through pipes to signal status changes
such as wakeup requests.

Sleeping slots are woken up periodically by the session. They can be forcefully woken up
by calling `session.wakeup()`.

