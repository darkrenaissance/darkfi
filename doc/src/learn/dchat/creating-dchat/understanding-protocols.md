### Understanding protocols

We now need to implement a custom protocol which defines how our chat
program interacts with the p2p network.

We've already interacted with several protocols already. Protocols
are automatically activated when nodes connect to eachother on the
p2p network. Here are examples of two protocols that every node runs
continuously in the background:

[ProtocolPing](../../../src/net/protocol/protocol_ping.rs): sends ping,
receives pong
[ProtocolAddress](../../../src/net/protocol/protocol_address.rs): receives
a get_address message, sends an address message

Under the hood, these protocols have a few similarities:

1. They create a subscription to a message type, such as Ping and Pong.
2. They implement [ProtocolBase](../../../src/net/protocol/protocol_base.rs),
DarkFi's generic protocol trait.
3. They run asynchronously using the
[ProtocolJobsManager](../../../src/net/protocol/protocol_jobs_manager.rs).
4. They hold a pointer to [Channel](../../../src/net/channel.rs) which
invokes the [MessageSubsystem](../../../src/net/message_subscriber).

This introduces several generic interfaces that we must use to build
our custom protocol. In particular:

1. The Message Subsystem

MessageSubsystem is a generic publish/subscribe class that can
dispatch any kind of message to a list of dispatchers. This is how we
can send and receive custom messages on the p2p network.

2. Message Subscription

A subscription to a message type. 

3. The Protocol Registry 

ProtocolRegistry takes any kind of generic protocol and initializes it. We
use it through the method register() which passes a protocol constructor
and a session bitflag which determines which sessions (outbound, inbound,
or seed) will run our protocol.

4. ProtocolJobsManager

An asynchronous job manager that spawns and stops tasks created by
protocols across the network.

5. ProtocolBase

A generic protocol trait that all protocols must implement.

