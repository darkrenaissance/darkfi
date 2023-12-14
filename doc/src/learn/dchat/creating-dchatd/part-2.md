# Part 2: Creating dchat

Now that we've deployed a local version of the p2p network, we can start
creating a custom protocol and message types that dchat will use to
send and receive messages across the network.

This section will cover:

* The `Message` type
* `Protocols` and the `ProtocolRegistry`
* The `MessageSubsystem`
* `MessageSubscription`
* `Channel`
* `JSON-RPC` `RequestHandler`
* `StoppableTask`
