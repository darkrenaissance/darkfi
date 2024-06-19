# Understanding Protocols

We now need to implement a custom protocol which defines how our chat
program interacts with the p2p network.

We've already interacted with several protocols already. Protocols
are automatically activated when nodes connect to eachother on the
p2p network. Here are examples of two protocols that every node runs
continuously in the background:

* [ProtocolPing](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/protocol/protocol_ping.rs):
sends `ping`, receives `pong`
* [ProtocolAddress](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/protocol/protocol_address.rs):
receives a `get_address` message, sends an `address` message

Under the hood, these protocols have a few similarities:

* They create a subscription to a message type, such as `ping` and `pong`.
* They implement [ProtocolBase](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/protocol/protocol_base.rs),
DarkFi's generic protocol trait.
* They run asynchronously using the
[ProtocolJobsManager](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/protocol/protocol_jobs_manager.rs).
* They hold a pointer to [Channel](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/channel.rs) which
invokes the [MessageSubsystem](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/net/message_subscriber.rs#L170).

This introduces several generic interfaces that we must use to build
our custom protocol. In particular:

**The Message Subsystem**

`MessageSubsystem` is a generic publish/subscribe class that contains
a list of `Message` dispatchers. A new dispatcher is created for every
`Message` type. These `Message` specific dispatchers maintain a list of
susbscribers that are subscribed to a particular `Message`.

**Message Subscription**

A subscription to a specific `Message` type. Handles receiving messages
on a subscription.

**Channel**

`Channel` is an async connection for communication between nodes. It is
also a powerful interface that exposes methods to the `MessageSubsystem`
and implements `MessageSubscription`.

**The Protocol Registry**

`ProtocolRegistry` is a registry of all protocols. We use it through the
method `register` which passes a protocol constructor and a session
`bitflag`. The `bitflag` specifies which sessions the protocol is created
for. The `ProtocolRegistry` then spawns new protocols for different channels
depending on the session.

TODO: document which protocols are included or excluded depending on the session.

**ProtocolJobsManager**

An asynchronous job manager that spawns and stops tasks. Its main
purpose is so a protocol can cleanly close all started jobs, through
the function `close_all_tasks`.  This way if the connection between
nodes is dropped and the channel closes, all protocols are also shutdown.

**ProtocolBase**

A generic protocol trait that all protocols must implement.
