# Structures

## EventId

Hash of `Event`

```rust
type EventId = [u8; 32];
```

## EventAction

The `Event` could have many actions according to the underlying data.

```rust
enum EventAction { ... };
```

## Event

| Description            | Data Type      | Comments                    |
|----------------------- | -------------- | --------------------------- |
| previous_event_hash    | `EventId`      | Hash of the previous `Event`|
| action                 | `EventAction`  | `Event`'s action            |
| timestamp              | u64            | `Event`'s timestamp         |

## EventNode

| Description    | Data Type              | Comments                                              |
|--------------- | ---------------------- | ----------------------------------------------------- |
| parent         | Option<`EventId`>      | Only current root has this set to None                |
| event          | `Event`                | The `Event` itself                                    |
| children       | Vec<`EventId`>         | The `Event`s which has parent as this `Event` hash    |

## Model

The `Model` consists of chains (`EventNodes`) structured as a tree; whereby, each chain has an `Event`-based
list. To maintain a strict order of chains, each `Event` depends on the hash of the previous `Event`.
All of the chains share a root `Event` to preserve the tree structure.

| Description   | Data Type                        | Comments                      |
|-------------- | -------------------------------- | ----------------------------- |
| current_root  | `EventId`                        | The root `Event` for the tree |
| orphans       | HashMap<`EventId`, `Event`>      | Recently added `Event`s       |
| event_map     | HashMap<`EventId`, `EventNode`>  | The actual tree               |
| events_queue  | `EventsQueue`                    | Communication channel         |

## View

The `View` checks the `Model` for new `Event`s and then dispatches these `Event`s to the clients.

`Event`s are sorted according to the timestamp attached to each `Event`.

| Description   | Data Type                     | Comments               |
|-------------- | ----------------------------- | ---------------------- |
| seen          | HashMap<`EventId`, `Event`>   | A list of `Event`s     |

## EventsQueue

The `EventsQueue` used to transport the event from `Model` to `View`.

The `Model` fills the `EventsQueue` with the new `Event`, while the `View` continuously
fetches `Event`s from queue.

# Architecture

Tau uses Model–view software architecture. All of the operations, main data structures,
and message handling from the network protocol happen on the `Model` side.
Further, this keeps the `View` independent of the `Model`
and allows the `View` to focus on receiving continuous updates from it.

## Add new `Event`

Upon receiving a new `Event` from the network protocol, the `Event` will be added to the
orphans list.

After the ancestor of the new orphan is found, the orphan `Event` will be added to the chain
according to its ancestor.

For example: in [Example1](#example1) below, an `Event` is added to the first chain if
its previous hash is Event-A1.

## Remove old leaves

Remove leaves which are too far from the head leaf (the leaf in the longest chain).

The depth difference from the common ancestor between a leaf to be removed and a head leaf
must be greater than `MAX_DEPTH`.

## Update the root

Finding the highest common ancestor for the leaves and assign it as the root
for the tree.

The highest common ancestor must have a height greater than `MAX_HEIGHT`.

## Example1

![data structure](../../assets/mv_event.png)
