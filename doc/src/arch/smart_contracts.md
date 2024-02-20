# Anonymous Smart Contracts

<!-- toc -->

Every full node is a **verifier**.

**Prover** is the person executing the smart contract function on
their secret witness data. They are also verifiers in our model.

Lets take a pseudocode smart contract:

```
contract Dao {
    # 1: the DAO's global state
    dao_bullas = DaoBulla[]
    proposal_bullas = ProposalBulla[]
    proposal_nulls = ProposalNull[]

    # 2. a public smart contract function
    #    there can be many of these
    fn mint(...) {
        ...
    }

    ...
}
```

## Important Invariants

1. The state of a contract (the contract member values) is globally
   readable but *only* writable by that contract's functions.
2. Transactions are atomic. If a subsequent contract function call
   fails then the earlier ones are also invalid. The entire tx will be
   rolled back.
3. `foo_contract::bar_func::validate::state_transition()` is able to
   access the entire transaction to perform validation on its structure.
   It might need to enforce requirements on the calldata of other
   function calls within the same tx. See `DAO::exec()`.

## Global Smart Contract State

Internally we represent this smart contract like this:

```rust
mod dao_contract {
    // Corresponds to 1. above, the global state
    struct State {
        dao_bullas: Vec<DaoBulla>,
        proposal_bullas: Vec<ProposalBulla>,
        proposal_nulls: Vec<ProposalNull>
    }

    // Corresponds to 2. mint()
    // Prover specific
    struct MintCall {
        ...
        // secret witness values for prover
        ...
    }

    impl MintCall {
        fn new(...) -> Self {
            ...
        }

        fn make() -> FuncCall {
            ...
        }
    }

    // Verifier code
    struct MintParams {
        ...
        // contains the function call data
        ...
    }
}
```

There is a pipeline where the prover runs `MintCall::make()` to create
the `MintParams` object that is then broadcast to the verifiers through
the p2p network.

The `CallData` usually is the public values exported from a ZK proof.
Essentially it is the data used by the verifier to check the function
call for `DAO::mint()`.

## Atomic Transactions

Transactions represent several function call invocations
that are atomic. If any function call fails, the entire tx is
rejected. Additionally some smart contracts might impose additional
conditions on the transaction's structure or other function calls
(such as their call data).

```rust
{{#include ../../../src/tx/mod.rs:transaction}}
```

Function calls represent mutations of the current active state to a new state.

```rust
{{#include ../../../src/sdk/src/tx.rs:contractcall}}
```

The `contract_id` corresponds to the top level module for the contract which
includes the global `State`.

The `func_id` of a function call corresponds to predefined objects
in the submodules:

* `Builder` creates the anonymized `CallData`. Ran by the prover.
* `CallData` is the parameters used by the anonymized function call
  invocation.
  Verifiers have this.
* `state_transition()` that runs the function call on the current state
  using the `CallData`.
* `apply()` commits the update to the current state taking it to the
  next state.

An example of a `contract_id` could represent `DAO` or `Money`.
Examples of `func_id` could represent `DAO::mint()` or
`Money::transfer()`.

Each function call invocation is ran using its own
`state_transition()` function.

```rust
mod dao_contract {
    ...

    // DAO::mint() in the smart contract pseudocode
    mod mint {
        ...

        fn state_transition(states: &StateRegistry, func_call_index: usize, parent_tx: &Transaction) -> Result<Update> {
            // we could also change the state_transition() function signature
            // so we pass the func_call itself in
            let func_call = parent_tx.func_calls[func_call_index];
            let call_data = func_call.call_data;
            // It's useful to have the func_call_index within parent_tx because
            // we might want to enforce that it appears at a certain index exactly.
            // So we know the tx is well formed.

            ...
        }
    }
}
```

The `state_transition()` has access to the entire atomic transaction to
enforce correctness. For example chaining of function calls is used by
the `DAO::exec()` smart contract function to execute moving money out
of the treasury using `Money::transfer()` within the same transaction.

Additionally `StateRegistry` gives smart contracts access to the
global states of all smart contracts on the network, which is needed
for some contracts.

Note that during this step, the state is *not* modified. Modification
happens after the `state_transition()` is run for all function
call invocations within the transaction. Assuming they all pass
successfully, the updates are then applied at the end. This ensures
atomicity property of transactions.

```rust
mod dao_contract {
    ...

    // DAO::mint() in the smart contract pseudocode
    mod mint {
        ...

        // StateRegistry is mutable
        fn apply(states: &mut StateRegistry, update: Update) {
            ...
        }
    }
}
```

The transaction verification pipeline roughly looks like this:

1. Loop through all function call invocations within the transaction:
    1. Lookup their respective `state_transition()` function based off
       their `contract_id` and `func_id`. The `contract_id` and
       `func_id` corresponds to the contract and specific function,
       such as `DAO::mint()`.
    2. Call the `state_transition()` function and store the update.
       Halt if this function fails.
2. Loop through all updates
    1. Lookup specific `apply()` function based off the `contract_id`
       and `func_id`.
    2. Call `apply(update)` to finalize the change.

## ZK Proofs and Signatures

Lets review again the format of transactions.

```rust
{{#include ../../../src/tx/mod.rs:transaction}}
```

And corresponding function calls.

```rust
{{#include ../../../src/sdk/src/tx.rs:contractcall}}
```

As we can see the ZK proofs and signatures are separate from the
actual `call_data` interpreted by `state_transition()`. They are
both automatically verified by the VM.

However for verification to work, the ZK proofs also need corresponding
public values, and the signatures need the public keys. We do this
by exporting these values. (TODO: link the code where this happens)

These methods export the required values needed for the ZK proofs
and signature verification from the actual call data itself.

For signature verification, the data we are verifying is simply
the entire transactions minus the actual signatures. That's why the
signatures are a separate top level field in the transaction.

