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
3. `foo_contract::bar_func::validate::process()` is able to
   access the entire transaction to perform validation on its structure.
   It might need to enforce requirements on the calldata of other
   function calls within the same tx. See `DAO::exec()`.

## Global Smart Contract State

Internally we could represent this smart contract like this:

```rust
mod dao_contract {
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
{{#include ../../../../src/tx/mod.rs:transaction}}
```

Function calls represent mutations of the current active state to a new
state.

```rust
{{#include ../../../../src/sdk/src/tx.rs:contractcall}}
```

The `contract_id` corresponds to the top level module for the contract
which includes the global `State`.

The `func_id` of a function call corresponds to predefined objects
in the submodules:

* `Builder` creates the anonymized `CallData`. Ran by the prover.
* `CallData` is the parameters used by the anonymized function call
  invocation.
  Verifiers have this.
* `process()` that runs the function call on the current state
  using the `CallData`.
* `apply()` commits the update to the current state taking it to the
  next state.

An example of a `contract_id` could represent `DAO` or `Money`.
Examples of `func_id` could represent `DAO::mint()` or
`Money::transfer()`.

Each function call invocation is ran using its own
`process()` function.

```rust
mod dao_contract {
    ...

    // DAO::mint() in the smart contract pseudocode
    mod mint {
        ...

        fn process(states: &StateRegistry, func_call_index: usize, parent_tx: &Transaction) -> Result<Update> {
            // we could also change the process() function signature
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

`process()` has access to the entire atomic transaction to
enforce correctness. For example combining of function calls is used by
the `DAO::exec()` smart contract function to execute moving money out
of the treasury using `Money::transfer()` within the same transaction.

Additionally smart contracts have access to the
global states of all smart contracts on the network, which is needed
for some contracts.

Note that during this step, the state is *not* modified. Modification
happens after the `process()` is run for all function
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
    1. Lookup their respective `process()` function based off
       their `contract_id`. The `contract_id` and
       `func_id` corresponds to the contract and specific function,
       such as `DAO::mint()`.
    2. Call the `process()` function and store the update.
       Halt if this function fails.
2. Loop through all updates
    1. Lookup specific `apply()` function based off the `contract_id`
       and `func_id`.
    2. Call `apply(update)` to finalize the change.

## ZK Proofs and Signatures

Lets review again the format of transactions.

```rust
{{#include ../../../../src/tx/mod.rs:transaction}}
```

And corresponding function calls.

```rust
{{#include ../../../../src/sdk/src/tx.rs:contractcall}}
```

As we can see the ZK proofs and signatures are separate from the
actual `call_data` interpreted by `process()`. They are
both automatically verified by the VM.

However for verification to work, the ZK proofs also need corresponding
public values, and the signatures need the public keys. We do this
by exporting these values. (TODO: link the code where this happens)

These methods export the required values needed for the ZK proofs
and signature verification from the actual call data itself.

For signature verification, the data we are verifying is simply
the entire transactions minus the actual signatures. That's why the
signatures are a separate top level field in the transaction.

This section of the book documents smart contract development.

## Invoking Contracts

In Solana and Ethereum, when invoking a contract, the call happens
directly at the site of calling. That means the calling contract is
responsible for constructing the params used to process the instruction.

In our case, it's more complicated since a smart contract function
invocation involves ZK proofs with `get_metadata()` that can be
verified in parallel. If we used the above model, we would first have
to execute `process()` before verifying the proofs or signatures.

Also arbitrary invocation allows arbitrary infinite recursion.

The alternative method which is close to what we're doing already, is
having the entire callgraph as a tree. Each `ContractCall`, now has a
field called `children: Vec<ContractCall>`.

```rust
pub struct ContractCall {
    /// ID of the contract invoked
    pub contract_id: ContractId,
    /// Call data passed to the contract
    pub data: Vec<u8>,

    /// Contract calls invoked by this one
    pub children: Vec<ContractCall>,
}
```

Let `n = len(children)`. Then inside the contract, we are expected to
call `invoke()` exactly `n` times.

```rust
    let (params, retdat) = invoke(function_id);
```

This doesn't actually invoke any function directly, but just iterates
to the next child call in the current call. We should iterate through
the entire list. If this doesn't happen, then there's a mismatch and
the call fails with an error.

This logic is handled completely inside the contract without needing
host functions.

The downside is that the entire calldata for a smart contract is
bundled in a tx, and isn't generated on the fly. This makes tx size
bigger. However since we need ZK proofs, I expect the calldata would
need to bundle the proofs for all invoked calls anyway.

Essentially the entire program trace is created ahead of time by the
"prover", and then the verifier simply checks the trace for
correctness. This can be done in parallel since we have all the data
ahead of time.

### Depending on State Changes

Another downside of this model is that state changes at the site of
invocation are not immediate.

Currently in DarkFi, we separate contract calls into 2-phases:
`process()` which verifies the calldata, and `update()` which takes a
state update from `process()` and writes the changes.

Host functions have permissions:

* `process()` is READONLY, which means state can only be read. For
  example it can use `db_get()` but *not* `db_set()`.
* `update()` is WRITEONLY. It can only write to the state. For example
  it can use `db_set()` but *not* `db_get()`.

Let `A`, `B` be smart contract functions. `A` calls `invoke(B)`. The
normal flow in Ethereum would be:

```
process(A) ->
    invoke(B) ->
        process(B) ->
        update(B) ->
update(A)
```

However with the model described, instead would be:

```
process(B) ->
update(B) ->
process(A) ->
update(A)
```

which simulates the previous trace.

~~State changes occur linearly after all `process()` calls have passed
successfully.~~

NOTE: we can iterate depth first through the tree to simulate the normal
calling pattern.

An upside of this strict separation, is that it makes reentrancy attacks
impossible. Say for example we have this code:

```js
contract VulnerableContract {
    function withdraw(amount) {
        // ...
        sender.call(amount);
        balances[sender] -= amount;
    }
}

contract AttackerContract {
    function receive(amount) {
        if balance(VulnerableContract) >= 1 {
            VulnerableContract.withdraw(1);
        }
    }
}
```

The main recommended way to mitigate these attacks is using the
'checks-effects-interactions' pattern[[1]](https://docs.soliditylang.org/en/v0.4.21/security-considerations.html#use-the-checks-effects-interactions-pattern)
[[2]](https://fravoll.github.io/solidity-patterns/checks_effects_interactions.html).
whereby the code is delineated into 3 strict parts.

```js
contract ChecksEffectsInteractions {
    // ...

    function withdraw(uint amount) public {
        require(balances[msg.sender] >= amount);

        balances[msg.sender] -= amount;

        msg.sender.transfer(amount);
    }
}
```

Interactions always occur last since they cause unknown effects.
Performing logic based off state changes from an interacting outside
contract (especially when user provided) is very risky.

With the model of `invoke()` given above, we do not have any
possibility of such an attack occurring.

## ABI

We can do this in Rust through clever use of the serializer. Basically
there is a special overlay provided for a Model which describes its
layout. The layout saves the field names and types. Later this can be
provided via a macro.

Then dynamically in the program code, the params can be (de)serialized
and inspected via this ABI overlay. This enables dynamic calls provided
by users to be supported in an elegant and simple way.

The ABI also aids in debugging since when the overlay is loaded, then
calldata can be inspected. Then we can inspect txs in Python, with
exported ABIs saved as JSON files per contract documenting each
function's params.

## Events

Custom apps will need to subscribe to blockchain txs, and be able to
respond to certain events. In Ethereum, there is a custom mechanism
called [events](https://docs.soliditylang.org/en/latest/abi-spec.html#events).
This allows smart contracts to
[return values to the UI](https://ethereum.stackexchange.com/questions/56879/can-anyone-explain-what-is-the-main-purpose-of-events-in-solidity-and-when-to-us).
Events are indexed in the database.

An equivalent mechanism in DarkFi, may be the ability to emit events
which wallets can subscribe to. As for storing them an indexed DB, we
already offer that functionality with `db_set()` during the update
phase.

The emitted event could consist of the ContractId/FunctionId, an
optional list of topics, and a binary blob.

Alternatively, wallets would have to listen to all calls of a specific
FunctionId. This allows wallets to only subscribe to some specific
aspect of those calls.

Solana by contrast allows the RPC to subscribe to accounts directly.
The equivalent in our case, is subscribing to `db_set()` calls. Wallets
can also receive these state changes and reflect them in their UI.
An [EventEmitter](https://github.com/solana-labs/solana/issues/14076)
was recently added to Solana.

Adding an explicit event emitter allows sending specific events used
for wallets. This makes dev on the UI side much easier. Additionally
the cost is low since any events emitted with no subscribers for that
contract or not matching the filter will just be immediately dropped.
