# DDoS Mitigation

## Improve Sync Algo

Right now the algo for syncing is very bad. It looks through every
available channel sequentially and performs these steps:

1. Get the current tips
2. Request their parents
3. Repeat

The entire process is the slowest sync algo. Lets think of a better
sync algo.

### Graph Locator

```
struct GraphLocator {
    tip_slots: Vec<EventHash>
}
```

Each tip slot contains a *sparse* path from an active tip to the
current root. Sparse here means the path is not complete.

The slot is: current tip, a continuous path for the next 5 and then
from there, gaps which double for every event.

When a node receives the graph locator, it can them compare it with
its own graph and immediately see where their graphs both diverge.

It then will sync from the divergence point.

### Phased Algo

Events are increasingly carrying more data. Therefore the graph
structure should be separated from the event data.

```
struct EventHeader {
    parents: Vec<EventHash>
    blob: EventDataHash
}
```

The sync algo then becomes:

1. **Locate phase:** Our node creates a GraphLocator and sends it to
   the remote. The remote uses this to see which events we're missing.
   The remote then sends a *capped* flood of Inventory objects.
    1. Our node may need to repeat sending the GraphLocator requests
       since the remote only sends a capped number of updates.
2. **Header sync phase:** Our node sends GetData request containing
   lists of the missing event header hashes. The remote responds with
   event header objects. Our node then links these up to build the
   missing graph structure.
3. **Data sync phase:** Lastly our node walks the graph and for every
   missing blob, requests the data. It does this backwards.

### Effect on DDoS

While improved sync is desirable, it still does not mitigate DDoS since
the attacker can easily double their resources. This merely makes nodes
more performant.

## Protocol Restriction

Proposal: add a global rate limit for messages. This is the easiest
and most direct fix we should apply right now.

There is a rolling window of 1 minute with a maximum of 100 events
allowed. Violators get `channel.ban()`.

Use a `VecDeque` of timestamps. `clean_bantimes()` will then do:

```rust
while let Some(ts) = bantimes.front() {
    if ts > NOW - 1 minute {
        break
    }
    let _ = bantimes.pop_front()
}
```

Now all events within the list should be within the last minute.
Push any new events to the list, then check:

```rust
if bantimes.len() > N {
    channel.ban();
}
```

This is the most immediate mitigation and overdue anyway.

Later we will make the 1 minute and N configurable with the static
event graph admin instance. For now, we can hardcode these values.

## Resource Manager

This will also be a big step towards alleviating DDoS. Check the p2p
doc for a design.

## RLN

The global limit is not ideal since it's... global. However RLN fixes
this since it provides a way for people to not be affected by the
rate-limit. Problem solved.

## Spam

Just regular spam. Solved by admin/mod features and outside the scope
of this doc. We need the static event graph though so we can lock
channels down during times of high activity or modify the posting rate
of certain keys (like public ones).

We already had the first step now with the `/ban` feature and admin
keys in the TOML.
