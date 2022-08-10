# Anonymous Smart Contracts

Every full node is a **verifier**.

**Prover** is the person executing the smart contract function on their secret witness data.
They are also verifiers in our model.

Lets take a pseudocode smart contract:

```rust
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

    // Corresponds to mint()
    mod mint {
        // Prover specific
        struct Builder {
            ...
            // secret witness values for prover
            ...
        }

        impl Builder {
            fn new(...) -> Self {
                ...
            }

            fn build() -> Box<FuncCallBase> {
                ...
            }
        }

        // Verifier code
        struct FuncCall {
            ...
        }
    }
}
```

There is a pipeline where the prover runs `Builder::build()` to create the `FuncCall` object that
is then broadcast to the verifiers through the p2p network.

## Atomic Transactions

Transactions represent several function call invocations that are atomic. If any function call fails,
then the entire tx is rejected. Additionally some smart contracts might impose additional conditions
on the transaction's structure or other function calls (such as their call data).

```rust
struct Transaction {
    func_calls: Vec<Box<FuncCallBase>>
}
```

Function calls represent mutations of the current active state to a new state.

The `ContractFuncId` of a function call corresponds predefined objects in the module:
* `Builder` creates the `FuncCall` invocation. Ran by the prover.
* `FuncCall` is the function call invocation that verifiers have access to.
* `state_transition()` that runs the function call on the current state.
* `apply()` commits the update to the current state taking it to the next state.

```rust
trait FuncCallBase {
    fn contract_func_id() -> ContractFuncId;
}
```

Each function call invocation is ran using its own `state_transition()` function.

```rust
mod dao_contract {
    ...

    // DAO::mint() in the smart contract pseudocode
    mod mint {
        ...

        fn state_transition(states: &StateRegistry, func_call_index: usize, parent_tx: &Transaction) -> Result<Update> {
            // we could pass the func_call, index and parent_tx also
            let (_, func_call) = parent_tx.func_calls[func_call_index];

            ...
        }
    }
}
```

The `state_transition()` has access to the entire atomic transaction to enforce correctness. For example
chaining of function calls is used by the `DAO::exec()` smart contract function to execute moving money out
of the treasury using `Money::pay()` within the same transaction.

Additionally `StateRegistry` gives smart contracts access to the global states of all smart contracts on the network,
which is needed for some contracts.

Note that during this step, the state is *not* modified. Modification happens after the `state_transition()` is run
for all function call invocations within the transaction. Assuming they all pass successfully, the updates are then
applied at the end. This ensures atomicity property of transactions.

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
    1. Lookup their respective `state_transition()` function based off their `contract_func_id`.
       The `contract_func_id` corresponds to the contract and specific function, such as `DAO::mint()`.
    2. Call the `state_transition()` function and store the update. Halt if this function fails.
2. Loop through all updates
    1. Lookup specific `apply()` function based off the `contract_func_id`.
    2. Call `apply(update)` to finalize the change.

## Parallelisation Techniques

Since verification is done through `state_transition()` which returns an update that is then committed
to the state using `apply()`, we can perform verify all transactions in a block in parallel.

To enable calling another transaction within the same block (such as flashloans), we can add a special
depends field within the tx that makes a tx wait on another tx before being allowed to verify.
This causes a small deanonymization to occur but brings a massive scalability benefit
to the entire system.

ZK proof verification should be done automatically by the system. Any proof that fails marks the entire
tx as invalid, and the tx is discarded. This should also be parallelized.
