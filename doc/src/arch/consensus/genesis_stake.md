Genesis stake
=============

The `Consensus::GenesisStake` function is used for bootstrapping the
Proof of Stake (PoS) network. Using this, we are able to create an
initial staking coin that participates in consensus and is able to
propose blocks. We can gather any number of these calls/transactions
and hardcode them into a constant genesis block, so anyone is able
to deterministically reproduce the genesis block and begin syncing
the blockchain.

The parameters to execute this function are a single clear input,
and a single anonymous output:

```rust,no_run,no_playground
{{#include ../../../../src/contract/consensus/src/model.rs:ConsensusGenesisStakeParams}}
```

For transparency, we use a clear input in order to show how many
tokens are initially minted at genesis, and an anonymous output
in order to anonymise the staker.

The ZK proof we use to prove the minting of the anonymous output
is the `ConsensusMint_V1` circuit:

```
{{#include ../../../../src/contract/consensus/proof/consensus_mint_v1.zk}}
```

Important to note here is that in the case of genesis, this mint will
have `epoch` set to 0 (zero) in order for these stakers to be able to
immediately propose blocks without a grace period in order to advance
the blockchain.

## Contract logic

### [`get_metadata()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/genesis_stake_v1.rs#L39)

In the `consensus_genesis_stake_get_metadata_v1` function, we gather
the public key used to verify the transaction signature from the clear
input, and we extract the necessary public inputs that go into the
`ConsensusMint_V1` proof verification.

### [`process_instruction()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/genesis_stake_v1.rs#L73)

In the `consensus_genesis_stake_process_instruction_v1` function, we
perform the state transition. We enforce that:

* The verifying slot for this function is actually the genesis slot (0)
* The _token ID_ from the clear input is the native network token
* The output coin was not already seen in the set of staked or unstaked coins
* The value commitments in the clear input and anon output match

If these checks pass, we create a state update with the output coin:

```rust,no_run,no_playground
{{#include ../../../../src/contract/consensus/src/model.rs:ConsensusGenesisStakeUpdate}}
```

### [`process_update()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/stake_v1.rs#L176)

For the state update, we use the `consensus_stake_process_update_v1`
function. This will simply take the state update produced by
`consensus_genesis_stake_process_instruction_v1` and add the coin to
the set of seen coins in the consensus state, and append it to the
Merkle tree of coins in the consensus Merkle tree of coins.
