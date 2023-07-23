## Hello Darkmap

In this part, we will walk through a simple Darkfi contract.

First, pull down the Darkmap repository, we'd use this example to understand how
to write a simple yet powerful Darkfi contract!

> Note: This book assumes basic familiarity with smart contracts and blockchain. 
> It's good if you are familiar with Rust. You'd still be able to follow along
> even if you aren't by inferring from the context.

```
git clone https://github.com/darkrenaissance/darkmap
```

In Darkfi, a contract is deployed as a wasm module. Rust has one of the best wasm support, so Darkmap is implemented in Rust.

## entrypoint.rs

Take a look at `Cargo.toml`, the package's configurations.

There is macro defining 4 entrypoints the contract runtime calls.
They are called in order:
1. init
1. metadata
1. exec
1. apply

```
darkfi_sdk::define_contract!(
    init:     init_contract,
    exec:     process_instruction,
    apply:    process_update,
    metadata: get_metadata
);
```

## Init

Init is responsible for 2 tasks:
1. from the zkas binary, compute and store the corresponding verifying key 
2. initialize any number of databases (e.g. for the contract's business logic)

```
fn init_contract(cid: ContractId, ix: &[u8]) -> ContractResult {
    
    // The way to load zkas binary as native contracts, it will probably be different regular contracts
    let set_v1_bincode = include_bytes!("../proof/set_v1.zk.bin");

    // Compute and store verifying key if it's not already stored
    zkas_db_set(&set_v1_bincode[..])?;

    // Check if a db is already initialized
    if db_lookup(cid, MAP_CONTRACT_ENTRIES_TREE).is_err() {
	// db for business logic
        db_init(cid, MAP_CONTRACT_ENTRIES_TREE)?;
    }

    Ok(())
}
```

### Under the hood

```
// https://github.com/darkrenaissance/darkfi/blob/85c53aa7b086652ed6d2428bf748f841485ee0e2/src/runtime/import/db.rs#L522

/// Only `deploy()` can call this. Given a zkas circuit, create a VerifyingKey and insert
/// them both into the db.
pub(crate) fn zkas_db_set(ctx: FunctionEnvMut<Env>, ptr: WasmPtr<u8>, len: u32) -> i32 {
```

## Metadata

## Exec

## Apply

## Others

* Where are the states stored?
	* There is no memory, there is calldata
* What are the runtime functions you can call?
	* db_init
	* db_lookup
	* zkas_db_set

