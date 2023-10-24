Proposal
========

The `Consensus::Proposal` function is used whenever a consensus
participant is able to produce a winning proof and wants to prove
they're the current consensus leader and are eligible to propose a
block. By itself, this smart contract has nothing to do with blocks
themself, it is up to the leader to choose which transactions to
include in the block they're proposing. The `Consensus::Proposal`
function simply serves as a way to verify that the block proposer is
indeed an eligible leader.

The parameters to execute this function are 1 anonymous input and 1
anonymous output, and other necessary metadata. Essentially we burn
the winning coin, and mint a new one in order to compete in further
slots. Every time a proposer wins the leader election, they have to
burn their competing coin, prove they're the winner, and then mint
a new coin that includes the block reward and is eligible to compete
in upcoming future slots.


```rust,no_run,no_playground
{{#include ../../../../src/contract/consensus/src/model.rs:ConsensusProposalParams}}
```

The ZK proof we use for this is a single circuit,
`ConsensusProposal_V1`:

```
{{#include ../../../../src/contract/consensus/proof/consensus_proposal_v1.zk}}
```

## Contract logic

### [`get_metadata()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/proposal_v1.rs#L46)

In the `consensus_proposal_get_metadata_v1` function, we gather
the necessary metadata that we use to verify the ZK proof and the
transaction signature. Inside this function, we also verify the
VRF proof executed by the proposer using a deterministic input and
the proposer's revealed public key. This public key is derived from
the input (burned) coin in ZK and is also used to sign the entire
transaction.

### [`process_instruction()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/proposal_v1.rs#L167)

In the `consensus_proposal_process_instruction_v1` function, we
perform the state transition. We enforce that:

* The timelock of the burned coin has passed and the coin is eligible to compete
* The Merkle inclusion proof of the burned coin is valid
* The revealed nullifier of the burned coin has not been seen before
* The value commitments match, this is done as `input+reward=output`
* The newly minted coin was not seen before

If these checks pass, we create a state update with the burned
nullifier and the minted coin:

```rust,no_run,no_playground
{{#include ../../../../src/contract/consensus/src/model.rs:ConsensusProposalUpdate}}
```

### [`process_update()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/proposal_v1.rs#L252)

For the state update, we use the `consensus_proposal_process_update_v1`
function. This takes the state update produced by
`consensus_proposal_process_instruction_v1` and appends the new
nullifier to the set of seen nullifiers, adds the minted coin to the
set of coins and appends it to the Merkle tree of all coins in the
consensus state.
