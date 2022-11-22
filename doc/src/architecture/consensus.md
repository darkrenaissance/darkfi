# Consensus

This section of the book describes how nodes participating in the DarkFi blockchain achieve consensus.

## Glossary

| Name                   | Description                                                                               |
|------------------------|-------------------------------------------------------------------------------------------|
| Consensus              | Algorithm for reaching Blockchain consensus between participating nodes                   |
| Node                   | darkfid daemon participating in the network                                               |
| Slot                   | Specified timeframe for block production, measured in seconds(default=20)                 |
| Epoch                  | Specified timeframe for blockchain events, measured in slots(default=10)                  |
| Leader                 | Block producer                                                                            |
| Unproposed Transaction | A transaction that exists in nodes memory pool, but have not yet been included in a block |
| P2P network            | Peer-To-Peer network all nodes communicate with each other                                |
| Finalization           | Append a Block and its content to canonical blockchain                                    |
| Fork                   | Chain of unfinalized blocks, starting by the last canonical block of blockchain           |

## Node main loop

As described in previous chapter, DarkFi is based on Ouroboros Crypsinous, therefore, to be able to produce a block,
at slot start each node checks if they are the slots' leader, based on the leader selection algorithm. 
If they succeed, they can produce a block containing unproposed transactions, by extending the largest known fork, 
and share it with rest nodes, by broadcasting it in the P2P network. 
Right before slot end, each node triggers the finalization check, to verify which blocks can be finalized,
also known as finalization sync period.

Pseudocode:
```
loop {
    wait_for_next_slot_start()

    if epoch_changed() {
        create_competing_coins()   
    }

    if is_slot_leader() {
        block = propose_block()
        p2p.broadcast_block(block)
    }

    wait_for_slot_end()

    chain_finalization()
}
```

## Listening for blocks

Concurently with the main loop, each node listens to new block proposals, and upon receiving them,
they try to extend a known fork, as described in next section.

## Fork extension

Since there can be more than one slot leaders, each node holds a set of known forks in memory.
When a node becomes leader, they extend the longest fork they hold.
Upon receiving a block, one of the following cases occur:

| Description                               | Handling                                                            |
|-------------------------------------------|---------------------------------------------------------------------|
| Block extends a known fork at its end     | Append block to fork                                                |
| Block extends a known fork not at its end | Create a new fork up to the extended block and append the new block |
| Block extends canonical blockchain        | Create a new fork containing the new block                          |
| Block doesn't extends any known chains    | Ignore block                                                        |

### Visual Examples

| Sympol        | Description                           |
|---------------|---------------------------------------|
| [C]           | Canonical(finalized) blockchain block |
| [C]--...--[C] | Sequence of canonical blocks          |
| [Ln]          | Block produced by Leader n            |
| +--           | Appending a block to fork             |
| /--           | Dropped fork                          |

Starting state:

                   |--[L0] <-- L0 fork
    [C]--...--[C]--|
                   |--[L1] <-- L1 fork

#### Case 1

New proposal received extending L0 fork:

                   |--[L0]+--[L2] <-- L0L2 fork
    [C]--...--[C]--|
                   |--[L1]        <-- L1 fork

#### Case 2

New proposal received extending L0L2 fork at [L0] slot:

                   |--[L0]--[L2]  <-- L0L2 fork
    [C]--...--[C]--|
                   |--[L0]+--[L3] <-- L0L3 fork
                   |
                   |--[L1]        <-- L1 fork

##### Case 3

New proposal received extending canonical:

                   |--[L0]--[L2] <-- L0L2 fork
    [C]--...--[C]--|
                   |--[L0]--[L3] <-- L0L3 fork
                   |
                   |--[L1]       <-- L1 fork
                   |
                   |+--[L4]      <-- L4 fork


## Finalization

When finalization sync period kicks in, each node finds the longest fork chain it holds, that is at least 3 blocks long,
without any other fock chain with same length.
If such a fork chain exists, nodes finalizes(appends to canonical blockchain) all proposed blocks up to the last one.
When a fork chain gets finalized, rest fork chains are removed from nodes memory pool.
This practically means, that while we have competting(same length) fork chains,
no finalization occurs until a slot with a single leader happens.

We continue Case 3 from previous sector to visualize this logic.
On slot 5, node observes 2 proposals, one extending L0L2 fork and one extending L0L3 fork:

                   |--[L0]--[L2]+--[L5a] <-- L0L2L5a fork
    [C]--...--[C]--|
                   |--[L0]--[L3]+--[L5b] <-- L0L3L5b fork
                   |
                   |--[L1]               <-- L1 fork
                   |
                   |--[L4]               <-- L4 fork

Finalization cannot occur, since we have two competting fork chains.
On next slot, node only observers 1 proposal, extending L0L3L5b fork:

                   |--[L0]--[L2]--[L5a]        <-- L0L2L5a fork
    [C]--...--[C]--|
                   |--[L0]--[L3]--[L5b]+--[L6] <-- L0L3L5bL6 fork
                   |
                   |--[L1]                     <-- L1 fork
                   |
                   |--[L4]                     <-- L4 fork

When finalization sync period starts, node sees that it can finalize fork L0L3L5bL6 and drop rest forks:

                   |/--[L0]--[L2]--[L5a]      <-- L0L2L5a fork
    [C]--...--[C]--|
                   |--[L0]--[L3]--[L5b]--[L6] <-- L0L3L5bL6 fork
                   |
                   |/--[L1]                   <-- L1 fork
                   |
                   |/--[L4]                   <-- L4 fork


resulting in the following state:

    [C]--...--[C]--|--[L6]

where canonical contains blocks L0, L3 and L5b from L0L3L56L6 fork.

