## DarkFi-Wasm Runtime Interface

The execution of smart contracts is performed within a Wasm VM, specifically
[`Wasmer`](https://docs.rs/wasmer/latest/wasmer/index.html). This environment
is limited in terms of the data types that can be used. For the most part,
it is only possible to operate on values defined by [`wasmer::Value`](https://docs.rs/wasmer/latest/wasmer/enum.Value.html).

This creates the need for code that allows us to translate
simple `wasmer::Values` into higher-order data types that are more useful
(e.g. enums, Results, custom Errors, and so on). 

## Interactions with Wasm

_Relevant code for this section can be found in src/runtime and src/sdk/src/._

### Smart Contracts

Smart contracts can only interact with Wasm via functions that map to
one of four 'contract sections' (see also the `ContractSection` enum). They are:

* `initialize`
* `entrypoint`
* `update`
* `metadata`

Business logic for a contract is contained in functions that are accessible
via the contract's `entrypoint`. Data is sent via the `payload` parameter.

All of the functions associated with the contract sections are processed by the `call()`
function in the runtime. It is this function that acts as the interface between
lower-level Wasm functionality and the APIs that are provided to contract
developers.

### DarkFi SDK

Wasm operations are also possible via APIs defined in the DarkFi SDK.
These work as foreign functions, making use of Rust's `unsafe` and `extern`
features to interface with Wasm from Rust.

Smart contracts in DarkFi use these lower-level Wasm functions work with state.
Therfore, smart contracts must also verify these return values to ensure that
errors are properly handled.

Relevant code: 
* `src/sdk/src/util.rs`
* `src/contracts/`

## Exit codes and type casting in the context of the Wasm runtime

As of now (Nov. 2023), there is not a single mechanism in the codebase to translate integer values
to custom Errors. As such, it is done in an ad-hoc manner in different locations. This
is an area of future work -- for now, there are manual conventions that should
be followed to reduce potential bugs.

### Exit Codes
The [Wasm functions](https://docs.rs/wasmer/latest/wasmer/#functions) are configured 
in `src/runtime/vm_runtime.rs` and stored in the Wasmer Instance as function imports. 

Exit codes should follow this convention:

* Negative value --> An error has occurred.
* `0` --> successful execution with no return data
* Positive value --> Successful execution. The value signifies returned data, e.g. an offset in a Vector

Negative values can be mapped to custom Errors, such as ContractErrors, in order to
provide more information as well as to be handled using e.g. `match` arms.

Relevant code:
* `src/sdk/src/error.rs`
* `src/runtime/import/`

### Type Casting

In most cases, `i64` is the return type for functions that interface with the wasm runtime.
It should be understood that the values stored in these datatypes are `u32`. 

Any conversion between types should be done with extreme care
to avoid issues such as integer overflow, underflow, or truncation. Return values
should be checked and errors should be handled properly. Further work is needed
to ensure that the code returns an error or panics if a value is found to be outside
of the range of `u32`.

Note also that [`wasmer::Value`s do not actually have a concept of being 
"signed"](https://docs.rs/wasmer/latest/wasmer/enum.Value.html#variants) (i.e. negative).

