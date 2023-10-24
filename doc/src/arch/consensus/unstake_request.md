Unstake request
===============

The `Consensus::UnstakeRequest` function is used when a consensus
participant wants to exit participation and plans to unstake their
staked coin. What the user is essentially doing here is burning
their coin they have been using for consensus participation,
and minting a new coin that isn't able to compete anymore, and is
timelocked for a predefined amount of time. This new coin then has to
wait until the timelock is expired, and then it can be used in the
[`Unstake`](unstake.md) function in order to be redeemed back into
the _Money_ state.

The parameters to execute this function are 1 anonymous input and 1
anonymous output:

```rust,no_run,no_playground
{{#include ../../../../src/contract/consensus/src/model.rs:ConsensusUnstakeRequestParams}}
```

In this function, we have two ZK proofs, `ConsensusBurn_V1` and
`ConsensusMint_V1`:

```
{{#include ../../../../src/contract/consensus/proof/consensus_burn_v1.zk}}
```

```
{{#include ../../../../src/contract/consensus/proof/consensus_mint_v1.zk}}
```

## Contract logic

### [`get_metadata()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/unstake_request_v1.rs#L43)

In the `consensus_unstake_request_get_metadata_v1` function, we gather
the public inputs necessary to verify the given ZK proofs. It's pretty
straightforward, and more or less the same as other `get_metadata`
functions in this smart contract.

### [`process_instruction()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/unstake_request_v1.rs#L99)

We perform the state transition in
`consensus_unstake_request_process_instruction_v1`. We enforce that:

* The timelock of the burned coin has passed and the coin is eligible for unstaking
* The Merkle inclusion proof of the burned coin is valid
* The revealed nullifier of the burned coin has not been seen before
* The input and output value commitments are the same
* The output/minted coin has not been seen before

When this is done, and everything passes, we create a state update
with the burned nullifier and the minted coin. Here we use the same
parameters like we do in [`Proposal`](proposal.md) - a nullifier and
a coin:

```rust,no_run,no_playground
{{#include ../../../../src/contract/consensus/src/model.rs:ConsensusProposalUpdate}}
```

### [`process_update()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/unstake_request_v1.rs#L174)

For the state update, we use the
`consensus_unstake_request_process_update_v1`
function. This takes the state update produced by
`consensus_unstake_request_process_instruction_v1`. With it, we
append the revealed nullifier to the set of seen nullifiers. The
minted _coin_, in this case however, does _not_ get added to the
Merkle tree of staked coins. Instead, we add it to the Merkle tree
of **unstaked** coins where it lives in a separate state. By doing
this, we essentially disallow the new coin to compete in consensus
again because in that state it does not exist. It only exists in the
unstaked state, and as such can only be operated with other functions
that actually read from this state - namely [`Unstake`](unstake.md)
