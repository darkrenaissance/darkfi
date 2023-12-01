# Smart Contracts on DarkFi

This section of the book documents smart contract development.

## Wishlist

* Explicit FunctionId
* Invoke function from another function
* Function params use an ABI which is introspectable
    * This could be done using a separate schema which is loaded.
      See [Solidity's ABI spec](https://docs.soliditylang.org/en/latest/abi-spec.html).
    * This could be used to build the param args like in python with `**kwargs`.
* Backtrace accessible by functions, so they can check the parent caller.

## Invoking Contracts

In Solana and Ethereum, when invoking a contract, the call happens directly
at the site of calling. That means the calling contract is responsible for
constructing the params used to process the instruction.

In our case, it's more complicated since a smart contract function invocation
involves ZK proofs with `get_metadata()` that can be verified in parallel.
If we used the above model, we would first have to execute
`process_instruction()` before verifying the proofs or signatures.

Also arbitrary invocation allows arbitrary infinite recursion.

The alternative method which is close to what we're doing already, is having
the entire callgraph as a tree. Each `ContractCall`, now has a field called
`children: Vec<ContractCall>`.

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
    let call = invoke(function_id);
```

If this doesn't happen, then there's a mismatch and the call fails with an
error.

This logic is handled completely inside the contract without needing host
functions.

The downside is that the entire calldata for a smart contract is bundled
in a tx, and isn't generated on the fly. This makes tx size bigger.
However since we need ZK proofs, I expect the calldata would need to
bundle the proofs for all invoked calls anyway.

## ABI

We can do this in Rust through clever use of the serializer. Basically there
is a special overlay provided for a Model which describes its layout.
The layout saves the field names and types. Later this can be provided
via a macro.

Then dynamically in the program code, the params can be serialized/deserialized
and inspected via this ABI overlay. This enables dynamic calls provided by
users to be supported in an elegant and simple way.

