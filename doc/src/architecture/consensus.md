# Consensus

This section of the book describes how nodes participating in the DarkFi
blockchain achieve consensus.

## Glossary

| Name                   | Description                                                                               |
|------------------------|-------------------------------------------------------------------------------------------|
| Consensus              | Algorithm for reaching blockchain consensus between participating nodes                   |
| Node                   | darkfid daemon participating in the network                                               |
| Slot                   | Specified timeframe for block production, measured in seconds(default=90)                 |
| Epoch                  | Specified timeframe for blockchain events, measured in slots(default=10)                  |
| Leader                 | Block producer                                                                            |
| Unproposed Transaction | Transaction that exists in the memory pool but has not yet been included in a block       |
| Block proposal         | Block that has not yet been appended onto the canonical blockchain                        |
| P2P network            | Peer-to-peer network on which nodes communicate with eachother                            |
| Finalization           | State achieved when a block and its contents are appended to the canonical blockchain     |
| Fork                   | Chain of block proposals that begins with the last block of the canonical blockchain      |

## Node main loop

As described in previous chapter, DarkFi is based on Ouroboros
Crypsinous. Therefore, block production involves the following steps:

At the start of every slot, each node runs the leader selection algorithm
to determine if they are the slot's leader. If successful, they can
produce a block containing unproposed transactions. This block is then
appended to the largest known fork and shared with rest of the nodes on
the P2P network as a block proposal.

Before the end of every slot each node triggers a _finalization check_,
to verify which block proposals can be finalized onto the canonical
blockchain. This is also known as the finalization sync period.

Pseudocode:
```
loop {
    wait_for_next_slot_start()

    if is_slot_leader() {
        block = propose_block()
        p2p.broadcast_block(block)
    }

    wait_for_slot_end()

    chain_finalization()
}
```

## Listening for blocks

Each node listens to new block proposals concurrently with the main
loop. Upon receiving block proposals, nodes try to extend the proposals
onto a fork that they hold in memory. This process is described in the
next section.

## Fork extension

Since there can be more than one slot leader, each node holds a set of
known forks in memory.  When a node becomes a leader, they extend the
longest fork they hold. 

Upon receiving a block, one of the following cases may occur:

| Description                               | Handling                                                            |
|-------------------------------------------|---------------------------------------------------------------------|
| Block extends a known fork at its end     | Append block to fork                                                |
| Block extends a known fork not at its end | Create a new fork up to the extended block and append the new block |
| Block extends canonical blockchain        | Create a new fork containing the new block                          |
| Block doesn't extend any known chain      | Ignore block                                                        |

### Visual Examples

| Sympol        | Description                            |
|---------------|----------------------------------------|
| [C]           | Canonical(finalized) blockchain block  |
| [C]--...--[C] | Sequence of canonical blocks           |
| [Ln]          | Proposal produced by Leader n          |
| Fn            | Fork name to identify them in examples |
| +--           | Appending a block to fork              |
| /--           | Dropped fork                           |

Starting state:

                   |--[L0] <-- F0
    [C]--...--[C]--|
                   |--[L1] <-- F1

#### Case 1

Extending F0 fork with a new block proposal:

                   |--[L0]+--[L2] <-- F0
    [C]--...--[C]--|
                   |--[L1]        <-- F1

#### Case 2

Extending F0 fork at [L0] slot with a new block proposal, creating a new fork chain:

                   |--[L0]--[L2]   <-- F0
    [C]--...--[C]--|
                   |--[L1]         <-- F1
                   |
                   |+--[L0]+--[L3] <-- F2

##### Case 3

Extending the canonical blockchain with a new block proposal:

                   |--[L0]--[L2] <-- F0
    [C]--...--[C]--|
                   |--[L1]       <-- F1
                   |
                   |--[L0]--[L3] <-- F2
                   |
                   |+--[L4]      <-- F3


## Finalization

When the finalization sync period kicks in, each node looks up the longest
fork chain it holds. There must be no other fork chain with same length.
If such a fork chain exists, nodes finalize all block proposals by appending them
to the canonical blockchain.

Once finalized, all fork chains are removed from the memory pool.
Practically this means that no finalization can occur while there are
competing fork chains of the same length. In such a case, finalization
can only occur when we have a a slot with a single leader.

We continue Case 3 from the previous section to visualize this logic.
On slot 5, a node observes 2 proposals. One extends the F0 fork,
and the other extends the F2 fork:

                   |--[L0]--[L2]+--[L5a] <-- F0
    [C]--...--[C]--|
                   |--[L1]               <-- F1
                   |
                   |--[L0]--[L3]+--[L5b] <-- F2
                   |
                   |--[L4]               <-- F3

Since we have two competing fork chains finalization cannot occur.

On next slot, a node only observes 1 proposal. So it extends the
F2 fork:

                   |--[L0]--[L2]--[L5a]        <-- F0
    [C]--...--[C]--|
                   |--[L1]                     <-- F1
                   |
                   |--[L0]--[L3]--[L5b]+--[L6] <-- F2
                   |
                   |--[L4]                     <-- F3

When the finalization sync period starts, the node finalizes fork
F2 and all other forks get dropped:

                   |/--[L0]--[L2]--[L5a]      <-- F0
    [C]--...--[C]--|
                   |/--[L1]                   <-- F1
                   |
                   |--[L0]--[L3]--[L5b]--[L6] <-- F2
                   |
                   |/--[L4]                   <-- F3

The canonical blockchain now contains blocks L0, L3, L5b and L6 from fork F2.

