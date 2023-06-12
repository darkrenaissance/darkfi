Unstake
=======

The `Consensus::Unstake` and `Money::Unstake` functions are used in
order to fully exit from the consensus participation and move back
the staked funds into the _Money_ state.

The _Unstake_ transaction consists of two contract calls, calling the
above mentioned functions. The parameters, respectively, are:

```rust,no_run,no_playground
{{#include ../../../../src/contract/money/src/model.rs:ConsensusUnstakeParams}}

{{#include ../../../../src/contract/money/src/model.rs:MoneyUnstakeParams}}
```

These two contract calls need to happen atomically, meaning they should
be part of a single transaction being executed on the network.  On a
high level, what is happening in the _unstake_ process is burning the
coin previously created through [`UnstakeRequest`](unstake_request.md)
in the _Consensus_ state and minting a new coin in the _Money_ state
where it can then again be used for other functionality outside
of consensus.

The contract calls execute in sequence:

1. `Consensus::Unstake`
2. `Money::Unstake`

The ZK proof we use to prove burning of the coin in _Consensus_ is the
`ConsensusBurn_V1` circuit:

```
{{#include ../../../../src/contract/consensus/proof/consensus_burn_v1.zk}}
```

The ZK proof we use to prove minting of the coin in _Money_ is the
`Mint_V1` circuit:

```
{{#include ../../../../src/contract/money/proof/mint_v1.zk}}
```

## Contract logic

### [`Consensus::get_metadata()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/unstake_v1.rs#L39)

In the `consensus_unstake_get_metadata_v1` function, we gather the
public inputs necessary to verify the `ConsensusBurn_V1` ZK proof,
and additionally the public key used to verify the transaction
signature. This pubkey is also derived and enforced in ZK.

### [`Consensus::process_instruction()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/unstake_v1.rs#L84)

For the _Consensus_ state transition, we use the
`consensus_unstake_process_instruction_v1` function. We enforce that:

* The next `call_idx` is a call to the `Money::UnstakeV1` function
* The input in the params to the next function is the same as current input
* The timelock from [`UnstakeRequest`](unstake_request.md) has expired
* The input coin Merkle inclusion proof is valid
* The input nullifier was not published before

If these checks pass, we create a state update with the revealed
_nullifier_:

```rust,no_run,no_playground
{{#include ../../../../src/contract/money/src/model.rs:ConsensusUnstakeUpdate}}
```

### [`Consensus::process_update()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/consensus/src/entrypoint/unstake_v1.rs#L169)

For the _Consensus_ state update, we use the
`consensus_unstake_process_update_v1` function. This will simply
append the revealed _nullifier_ to the existing set of nullifiers in
order to prevent double-spending.

After the `Consensus::Unstake` state transition has passed, we move on
to executing the `Money::Unstake` state transition. This is supposed
to mint the new coin in the _Money_ state.

### [`Money::get_metadata()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/money/src/entrypoint/unstake_v1.rs#L41)

In the `money_unstake_get_metadata_v1` function, we gather the public
inputs necessary to verify the `Mint_V1` ZK proof. It is not necessary
to grab any public keys for signature verification, as they're already
collected in `Consensus::get_metadata()`.

### [`Money::process_instruction()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/money/src/entrypoint/unstake_v1.rs#L79)

In the `money_unstake_process_instruction_v1` function, we perform
the state transition. We enforce that:

* The previous `call_idx` is a call to the `Consensus::UnstakeV1` function
* The token pedersen commitment is a commitment to the native network token
* The value pedersen commitments in the input and output match
* The input coin Merkle inclusion proof is valid for _Consensus_
* The input nullifier was published in _Consensus_
* The output coin was not seen before in the set of coins in _Money_

If these checks pass, we create a state update with the revealed
minted coin:

```rust,no_run,no_playground
{{#include ../../../../src/contract/money/src/model.rs:MoneyUnstakeUpdate}}
```

### [`Money::process_update()`](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/money/src/entrypoint/unstake_v1.rs#L194)

In `money_unstake_process_update_v1` we simply append the newly minted
coin to the set of seen coins in _Money_, and we add it to the Merkle
tree of coins in _Money_ so further inclusion proofs can be validated.
