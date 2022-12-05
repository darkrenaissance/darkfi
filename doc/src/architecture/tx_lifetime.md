# Transaction behaviour

_(Temporary document, to be integrated into other docs)_

In our network context, we have two types of nodes.

1. Consensus Participant (`CP`)
2. Consensus Spectator (non-participant) (`CS`)

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
8. `CP` proposes a block finalization containing $tx$
9. `CP` writes the state transition update of $tx$ to their chain
10. `CP` removes $tx$ from their `mempool`
11. `CP` broadcasts a finalization proposal
12. `CS` receives the proposal and validates transactions
13. `CS` writes the state updates to their chain
14. `CS` removes $tx$ from their `mempool`

In the above context, `CS` acts as a relayer for transactions in
order to help out that transactions reach `CP`.

To avoid spam attacks, `CS` should keep $tx$ in their mempool for some
period of time, and then prune it.

## Real-world simulation with non-instant finality and forks

Write here.
