# Network Protocol

The protocol checks that `Event`s properly broadcast through the
network before adding `Event`s to the `Model`.

The read_confirms inside each `Event` indicate how many times the
`Event` has been read from other nodes in the network.

The protocol classifies the `Event`s by their state:

```text
Unread: read_confirms < MAX_CONFIRMS
Read:	read_confirms >= MAX_CONFIRMS
```

## Inv

Inventory vectors notify other nodes about objects they have or data
which is being requested.

| Description   | Data Type            | Comments                   |
|-------------- | -------------------- | -------------------------- |
| invs          | `Vec<[u8; 32]>`      | Inventory items            |

### Receiving an `Inv` message

Allows a node to advertise its knowledge of one or more objects. It can
be received unsolicited or in reply to `getevents`.

An `Inv` message is a confirmation from a node in the network that the
`Event` has been read.

Confirmation for an `Event` does not exist in the `UnreadEvents` list.
Instead, the protocol sends a `GetData` message to request the missing
`Event`.

The protocol updates the `Event` in the `UnreadEvents` list by
increasing the read_confirms by one.

The updated `Event` state changes to read when the read_confirms exceed
`MAX_CONFIRMS`. Then, the `UnreadEvents` list removes the `Event` and
adds it to the `Model`.

The protocol rebroadcasts the received `Inv` to the network.

### Sending an `Inv` message

Upon receiving an `Event` with unread status from the network, the
protocol sends back an `Inv` message to confirm that the `Event` has
been read.

## GetData

| Description   | Data Type            | Comments                   |
|-------------- | -------------------- | -------------------------- |
| events        | Vec<`EventId`>       | A list of `EventId`s       |

### Receiving a `GetData` message

The protocol searches in both `Model` and `UnreadEvents` for the
requested `Event`s in `GetData` message.

## UnreadEvents

| Description | Data Type                   | Comments                                                                             |
|-------------|---------------------------- | -------------------------------------------------------------------------------------|
| Messages    | HashMap<`EventId`, `Event`> | Hold all the `Event`s that have broadcasted to other nodes but haven't confirmed yet |

### Add new `Event` to `UnreadEvents`

To add an `Event` to `UnreadEvents`, the protocol first must check the
 validity of `Event`.

The `Event` is not valid in the network if it's either too far in the
future or in the past.

### Updating `UnreadEvents` list

The protocol continuously broadcasts unread `Event`s to the network,
after a certain period of time (`SEND_UNREAD_EVENTS_INTERVAL`),
until the state of `Event` updates to read.

## SyncEvent

| Description | Data Type       | Comments                      |
|-------------|---------------- |------------------------------ |
| Leaves      | Vec<`EventId`>  | Hash of `Event`s              |

### Synchronization

To achieve complete synchronization between nodes, the protocol sends a
`SyncEvent` message every 2 seconds to other nodes in the network.

The `SyncEvent` contains the hashes of `Event`s set in the leaves of
`Model`'s tree.

On receiving `SyncEvent` message, the leaves in `SyncEvent` should
match the leaves in the `Model`'s tree; otherwise, the protocol sends
`Event`s which are the children of `Event`s in `SyncEvent`.

## Seen

This prevents receiving duplicate objects.
The list contains only 2^16 ids.

| Description | Data Type       | Comments                      |
| ----------- | --------------- |------------------------------ |
| Ids         | Vec<`ObjectId`> | Contains objects ids          |

## Receiving a new `Event`

The new received `Event` with unread status is added to the
`UnreadEvents` buffer after increasing the read_confirms by one.

The `Event` with read status is added to the `Model`.

The protocol broadcasts the received `Event` to the network, again.
This ensures all nodes in the network get the Event.

## Sending an `Event`

A new created `Event` has unread status with read_confirms equal to 0.

The protocol broadcasts the `Event` to the network after adding it to
the `UnreadEvents`.

## Add new `Event` to `Model`

For the `Event` to be successfully added to the `Model`, the protocol
checks if the previous `Event`'s hash inside the `Event` exists in the
`Model`.

In case the previous `Event` check fails, the protocol
sends a `GetData` message requesting the previous `Event`.
