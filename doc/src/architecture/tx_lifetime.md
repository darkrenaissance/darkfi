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

## Real-world simulation with non-instant finality

The lifetime of a transaction $tx$ that passes verification and whose
state transition is pending to be applied on top of the finalized (canonical)
chain:

1. User creates a transaction $tx$
2. User broadcasts $tx$ to `CS`
3. `CS` validates $tx$ state transition
4. $tx$ enters `CS` `mempool`
5. `CS` broadcasts $tx$ to `CP`
6. `CP` validates $tx$ state transition
7. $tx$ enters `CP` `mempool`
8. `CP` proposes a block proposal containing $tx$
9. `CP` proposes more block proposals
10. When proposals can be finalized, `CP` validates all their transactions
in sequence
11. `CP` writes the state transition update of $tx$ to their chain
12. `CP` removes $tx$ from their `mempool`
13. `CP` broadcasts the finalizated proposals sequence
14. `CS` receives the proposals sequence and validates transactions
15. `CS` writes the state updates to their chain
16. `CS` removes $tx$ from their `mempool`

## Real-world simulation with non-instant finality, forks and multiple `CP` nodes

The lifetime of a transaction $tx$ that passes verifications and whose
state transition is pending to be applied on top of the finalized (canonical)
chain:

1. User creates a transaction $tx$
2. User broadcasts $tx$ to `CS`
3. `CS` validates $tx$ state transition against canonical chain state
4. $tx$ enters `CS` `mempool`
5. `CS` broadcasts $tx$ to `CP`
6. `CP` validates $tx$ state transition against all known fork states
7. $tx$ enters `CP` `mempool`
8. `CP` broadcasts $tx$ to rest `CP` nodes
9. Slot producer `CP` (`SCP`) node finds which fork to extend
10. `SCP` validates all unproposed transactions in its `mempool` in sequence,
against extended fork state, discarding invalid
11. `SCP` creates a block proposal containing $tx$ extending the fork
12. `CP` receives block proposal and validates its transactions against
the extended fork state
13. `SCP` proposes more block proposals extending a fork state
14. When a fork can be finalized, `CP` validates all its proposals
transactions in sequence, against canonical state
15. `CP` writes the state transition update of $tx$ to their chain
16. `CP` removes $tx$ from their `mempool`
17. `CP` drop rest forks and keeps only the finalized one
18. `CP` broadcasts the finalizated proposals sequence
19. `CS` receives the proposals sequence and validates transactions
20. `CS` writes the state updates to their chain
21. `CS` removes $tx$ from their `mempool`

`CP` will keep $tx$ in its `mempool` as long as it is a valid state transition
for any fork(including canonical) or it get finalized.

Unproposed transactions refers to all $tx$ not included in a proposal of any fork.

If a fork that can be finalized fails to validate all its transactions(14), it should be dropped.
