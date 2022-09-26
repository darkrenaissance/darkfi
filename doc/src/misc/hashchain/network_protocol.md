# Network Protocol

The protocol check that `Event`s have properly broadcasted through the
network before adding `Event`s to the `Model`. 

The read_confirms inside each `Event` indicate how many times the `Event` has 
been read from other nodes in the network.

The protocol classify the Events by their state:

	| Unread   |  read_confirms < `MAX_CONFIMRS`  | 
	| Read 	   |  read_confirms >= `MAX_CONFIMRS` | 

## Receiving a new `Event`

The new received `Event` with unread status gets add to the `UnreadMessages` buffer after
increasing the read_confirms by one. 

The `Event` with read status gets add to the `Model`.

The protocol broadcast the received `Event` to the network again, to ensure every nodes
in the network get the Event.

## Sending an `Event`

A new created `Event` has unread status with read_confirms equal to 0.

The protocol broadcast the `Event` to the network after adding it to the
`UnreadMessages`.

## Receiving an `Inv` message

An `Inv` message is a confirmation from a node in the network that the `Event`
has been read.

Confirmation for an `Event` not exist in the `UnreadMessages` list, 
A `GetData` message must send back to request the missing `Event`.

The protocol update the `Event` in the `UnreadMessages` list by increasing the
read_confirms by one.

The state for updated `Event` change to read when the read_confirms exceed
`MAX_CONFIMRS`, Then the `Event remove from the `UnreadMessages` list and add to the `Model`. 

The protocol rebroadcast the received `Inv` to the network.

## Sending an `Inv` message

On receiving an `Event` with unread status from the network, The protocol send back 
an `Inv` message to confirm that the `Event` has been read.

## Receiving a `GetData` message

The protocol search in both `Model` and `UnreadMessages` for requested `Event`
in `GetData` message.

## Add new `Event` to `Model` 

For the `Event` to be successfully add to the `Model`, the protocol check if
the previous `Event`'s hash inside the `Event` is exist in the `Model`.

In case the check for previous `Event` failed The protocol 
send a `GetData` message requesting the previous `Event`.

## Add new `Event` to `UnreadMessages` 

To add an `Event` to `UnreadMessages`, the protocol first must check the validity of
`Event`. 

The `Event` is not valid in the network if it's too far in the future from now,
or too far in the past from now.

## Updating `UnreadMessages` list

The protocol continually broadcast unread `Event` to the network 
after a certain period of time(`SEND_UNREAD_EVENTS_INTERVAL`), 
Until the state of `Event` updated to read.

## Synchronization

To achieve complete synchronization between nodes, the protocol send a
`SyncEvent` message every 2 seconds to other nodes in the network.

The `SyncEvent` contains the hashes of `Event`s set in the leaves of `Model`'s tree.

On receiving `SyncEvent` message, The leaves in `SyncEvent` should match the 
leaves in the `Model`'s tree, Otherwise the protocol send `Event`s which are the childern of
`Event`s in `SyncEvent` 

