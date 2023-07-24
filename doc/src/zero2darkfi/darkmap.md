## Darkmap

An immutable name registry. The important feature is that names can be
immutable.
From an end user perspective, they provide a dpath and get a value back.

For example:
```
provide: darkrenaissance::darkfi::v0_4_1
get:     0766e910aae7af482885d0a5b05ccb61ae7c1af4 (which is the commit for Darkfi v0.4.1, https://github.com/darkrenaissance/darkfi/commit/0766e910aae7af482885d0a5b05ccb61ae7c1af4)
```

Syntax:
```
  Colon means the key is locked to particular value.
  For example, the key v0_4_1 is locked to 0766e910aae7af482885d0a5b05ccb61ae7c1af4
  in the name registry that darkrenaissance:darkfi points to.
  It's helpful to know a tag always means the same commit.
	              v
darkrenaissance:darkfi:v0_4_1


  Dot means the key is not locked to a value. 
  It can be locked to a value later or be changed to a different value.
  For example, master (HEAD) currently maps to 85c53aa7b086652ed6d2428bf748f841485ee0e2,
  It's helpful because HEAD needs to change.
	              v
darkrenaissance:darkfi.master
```

Beyond the usual things one can do with a name registry e.g. naming website,
an immutable name provides strong security.
If the contract and blockchain are secure, the name doesn't change, not even the owner.

Being deployed on Darkfi also means there is no trace you own a name because gas payment
is anonymous.

## Contract implementation

> Note: This book assumes basic familiarity with smart contracts and blockchain. 
> It's good if you are familiar with Rust. You'd still be able to follow along
> even if you aren't by inferring from the context.

```
git clone https://github.com/darkrenaissance/darkmap
```

### Tool: wasm contract

In Darkfi, a contract is deployed as a wasm module. 
Rust has one of the best wasm support, so Darkmap is implemented in Rust.
In theory, any language that compiles to wasm can be used make a contract e.g. Zig.

### Tool: zkvm and zkas

You can make a ZK scheme where:
* a user computes a ZK proof locally and submit along the transaction
* the contract grants access to certain features only when it receives a valid proof

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


## Outline of book

* What is the problem?
* What are the tools?
	* zkbincode
	* wasm crate
	* transaction builder
	* test facility
