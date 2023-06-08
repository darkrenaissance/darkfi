Stake
=====

The `Money::Stake` and `Consensus::Stake` functions are used in order
to apply to become eligible for participation in the block proposal
process, commonly known as Consensus.

The _Stake_ transaction consists of two contract calls, calling the
above mentioned functions. The parameters, respectively, are:

```rust,no_run,no_playground
{{#include ../../../../src/contract/money/src/model.rs:MoneyStakeParams}}

{{#include ../../../../src/contract/money/src/model.rs:ConsensusStakeParams}}
```

These two contract calls need to happen atomically, meaning they
should be part of a single transaction being executed on the network.
On a high level, what is happening in the _stake_ process is burning
a coin in the state of _Money_ and minting a coin in the state of
_Consensus_ in order to start being able to participate in consensus
and propose blocks.

The contract calls execute in sequence:

1. `Money::Stake`
2. `Consensus::Stake`

The ZK proof we use to prove burning of the coin in _Money_ is the
`Burn_V1` circuit:

```
{{#include ../../../../src/contract/money/proof/burn_v1.zk}}
```

The ZK proof we use to prove minting of the coin in _Consensus_ is the
`ConsensusMint_V1` circuit:

```
{{#include ../../../../src/contract/consensus/proof/consensus_mint_v1.zk}}
```

## Contract logic

### [`Money::get_metadata()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/money/src/entrypoint/stake_v1.rs#L40)

In the `money_stake_get_metadata_v1` function, we gather the input
pubkey for signature verification, and extract necessary public inputs
for verifying the money burn ZK proof.

### [`Money::process_instruction()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/money/src/entrypoint/stake_v1.rs#L87)

In the `money_stake_process_instruction_v1` function, we perform the
state transition. We enforce that:

* The input `spend_hook` is 0 (zero) (for now we don't have protocol-owned stake)
* The input _token ID_ corresponds to the native network token (the commitment blind is revealed in the params)
* The input coin Merkle inclusion proof is valid
* The input nullifier was not published before
* The next `call_idx` is a call to the `Consensus::StakeV1` function
* The input in the params to the next function is the same as the current input

If these checks pass, we create a state update with the revealed
_nullifier_:

```rust,no_run,no_playground
{{#include ../../../../src/contract/money/src/model.rs:MoneyStakeUpdate}}
```

### [`Money::process_update()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/money/src/entrypoint/stake_v1.rs#L169)

For the _Money_ state update, we use the
`money_stake_process_update_v1` function. This will simply append
the revealed _nullifier_ to the existing set of nullifiers in order
to prevent double-spending.

After the `Money::Stake` state transition has passed, we move on to
executing the `Consensus::Stake` state transition. This is supposed
to mint the new coin in the _Consensus_ state.

### [`Consensus::get_metadata()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/stake_v1.rs#L41)

In `consensus_stake_get_metadata_v1` we grab the current epoch of
the slot where we're executing this contract call and use it as one
of the public inputs for the ZK proof of minting the new coin. This
essentially serves as a timelock where we can enforce a grace period
for this staked coin before it is able to start proposing blocks. More
information on this can be found in the [Proposal](proposal.md) page.
Additionally we extract the coin and the value commitment to use as
the proof's public inputs.

### [`Consensus::process_instruction()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/stake_v1.rs#L75)

In `consensus_stake_process_instruction_v1` we perform the state
transition. We enforce that:

* The previous `call_idx` is a call to `Money::StakeV1`
* The `Input` from the current call is the same as the `Input` from
  the previous call (essentially copying it)
* The value commitments in the `Input` and `ConsensusOutput` match
* The `Input` coin's Merkle inclusion proof is valid in the _Money_ state
* The input's _nullifier_ wasn't revealed before in the _Money_ state
* The `ConsensusOutput` coin hasn't existed in the _Consensus_ state before
* The `ConsensusOutput` coin hasn't existed in the _Unstaked Consensus_ state before

If these checks pass we create a state update with the minted coin
that is now considered staked in _Consensus_:

```rust,no_run,no_playground
{{#include ../../../../src/contract/money/src/model.rs:ConsensusStakeUpdate}}
```

### [`Consensus::process_update()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/stake_v1.rs#L176)

For the state update, we use the `consensus_stake_process_update_v1`
function. This takes the coin from the `ConsensusOutput` and adds
it to the set of staked coins, and appends it to the Merkle tree of
staked coins so participants are able to create inclusion proofs in
the future.
