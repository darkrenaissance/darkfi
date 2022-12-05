# Transaction behaviour

_(Temporary document, to be integrated into other docs)_

In our network context, we have two types of nodes.

1. Consensus Participant (`CP`)
2. Consensus Spectator (non-participant) (`CS`)

`CS` acts as a relayer for transactions in order to help out
that transactions reach `CP`.

To avoid spam attacks, `CS` should keep $tx$ in their mempool for some
period of time, and then prune it.

## Ideal simulation with instant finality

The lifetime of a transaction $tx$ that passes verification and whose
state transition can be applied on top of the finalized (canonical)
chain:

1. User creates a transaction $tx$
2. User broadcasts $tx$ to `CS` 
3. `CS` validates $tx$ state transition
4. $tx$ enters `CS` `mempool`
5. `CS` broadcasts $tx$ to `CP`
6. `CP` validates $tx$ state transition
7. $tx$ enters `CP` `mempool`
8. `CP` validates all transactions in its `mempool` in sequence
9. `CP` proposes a block finalization containing $tx$
10. `CP` writes the state transition update of $tx$ to their chain
11. `CP` removes $tx$ from their `mempool`
12. `CP` broadcasts the finalizated proposal
13. `CS` receives the proposal and validates transactions
14. `CS` writes the state updates to their chain
15. `CS` removes $tx$ from their `mempool`

## Real-world simulation with non-instant finality and forks

The lifetime of a transaction $tx$ that passes verification and whose
state transition is pending to be applied on top of the finalized (canonical)
chain:

1. User creates a transaction $tx$
2. User broadcasts $tx$ to `CS`
3. `CS` validates $tx$ state transition against canonical chain state
4. $tx$ enters `CS` `mempool`
5. `CS` broadcasts $tx$ to `CP`
6. `CP` validates $tx$ state transition against all known fork states
7. $tx$ enters `CP` `mempool`
8. `CP` finds which fork to extend
9. `CP` validates all unproposed transactions in its `mempool` in sequence,
against extended fork state, discarding invalid
10. `CP` proposes a block proposal containing $tx$ extending the fork
11. When/if fork can be finalized, `CP` validates all its proposals
transactions in sequence, against canonical state
12. `CP` writes the state transition update of $tx$ to their chain
13. `CP` removes $tx$ from their `mempool`
14. `CP` broadcasts the finalizated proposals sequence
15. `CS` receives the proposals sequence and validates transactions
16. `CS` writes the state updates to their chain
17. `CS` removes $tx$ from their `mempool`

`CP` will keep $tx$ in its `mempool` as long as it is a valid state transition
for any fork(including canonical) until it get finalized.

Unproposed transactions refers to all $tx$ not included in a proposal of any fork.

If a fork that can be finalized fails to validate all its transactions(11), it should be dropped.
