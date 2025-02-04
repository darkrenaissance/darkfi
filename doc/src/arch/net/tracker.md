# Tracker

Trackers are run by the community. They allow nodes to dynamically discover
swarms by querying the tracker for info.

Imagine you are in a channel on the chat. The chat may group media for that day
in a single swarm which is accessed by nodes in that channel. To access the
media, nodes will query the tracker, and then be able to spawn a unique `P2P`
instance.

Resource -> tracker -> P2P instance

## Creating a Network

Networks are spawned on the tracker in a lazy way. That is quering the tracker
will automatically spawn a network if not already available.

## Dropping Networks

Networks are auto-pruned when they become stale. Assume 1 week without updates
although this can be configured.

## Query Tracker

We have an identifier string, like `/darkirc/dev/1134/media`. Here `1134` refers
to the ID of the event graph instance.

Before creating the `P2P` instance, we must get the `Settings` struct.
We then query the tracker like this:

1. Send the tracker our external IP, and a port we listen on (optional - this is
   for inbound nodes only) together with the identifier.
2. If the tracker has no existing info for that identifier, then it will create
   a new table.
3. Add our node info to the table.
4. The tracker returns a list of nodes back to us.

This impl replaces the PeerDiscovery mechanism inside OutboundSession.

```
trackers = [
    # list of trackers to query for resources ...
]
```

## Role of Seeds

Seeds may continued to be used in place of trackers such as the main DarkIRC
event graph or darkfid where the networks are statically instanced.

For dynamic instance (swarming), we may eventually replace trackers with a swarm
network. Seed nodes will then be used to discover the swarm network.
Trackers allow p2p net discovery to be dynamic.

