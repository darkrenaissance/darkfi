# Concepts

## Transactions

Each *transaction* is an atomic state update containing several *contract calls*
organized in a tree.

A *contract call* is the call data and function ID that calls a specific
*contract function*.
Additionally associated with each call are proofs and signatures that
can be verified in any order.

```rust
{{#include ../../../src/tx/mod.rs:transaction}}
```

```rust
{{#include ../../../src/sdk/src/tx.rs:contractcall}}
```

The tree of structure for contract calls corresponds to invocation semantics,
but with the entire callgraph unrolled ahead of time.

## WASM VM

*Host* refers to the context which loads and calls the WASM contract code.

Contracts are compiled WASM binary code. The WASM engine is a sandboxed
environment with limited access to the host.

*WASM exports* are functions exported from WASM and callable by the WASM engine.

## Contract Sections

Contract operation is defined by *sections*. Each contract section is only
allowed to call certain [host functions](#host-functions).

**Example:** `exec()` may call `db_get()` but not `db_set()`, while `update()`
cannot call `db_get()`, but may call `db_set()`.

```rust
{{#include ../../../src/runtime/vm_runtime.rs:contract-section}}
```

For a list of permissions given to host functions, see the section [Host Functions](#host-functions).

## Contract Function IDs

Let $𝔽ₚ$ be defined as in the section [Pallas and Vesta](crypto-schemes.md#pallas-and-vesta).

Functions can be specified using a tuple $\t{FuncId} := (𝔽ₚ, 𝔹)$.

## Host Functions

Host functions give access to the executing node context external to the local
WASM scope.

The current list of functions are:

| Host function                      | Permission                     | Description                                 |
|------------------------------------|--------------------------------|---------------------------------------------|
| `db_init`                          | Deploy                         | Create a new database                       |
| `db_lookup`                        | Deploy, Exec, Metadata, Update | Lookup a database handle by name            |
| `db_set`                           | Deploy, Update                 | Set a value                                 |
| `db_del`                           | Deploy, Update                 | Remove a key                                |
| `db_get`                           | Deploy, Exec, Metadata         | Read a value from a key                     |
| `db_contains_key`                  | Deploy, Exec, Metadata, Update | Check if a given key exists                 |
| `zkas_db_set`                      | Deploy                         | Insert a new ZK circuit                     |
| `merkle_add`                       | Update                         | Add a leaf to a merkle tree                 |
| `set_return_data`                  | Exec, Metadata                 | Used for returning data to the host         |
| `get_verifying_block_height`       | Deploy, Exec, Metadata, Update | Runtime verifying block height              |
| `get_verifying_block_height_epoch` | Deploy, Exec, Metadata, Update | Runtime verifying block height epoch        |
| `get_blockchain_time`              | Deploy, Exec, Metadata, Update | Current blockchain (last block's) timestamp |
| `get_last_block_info`              | Exec                           | Last block's info, used in VRF proofs       |

