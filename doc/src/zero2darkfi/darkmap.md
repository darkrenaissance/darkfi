We are going to walk through a simple (1 private field) contract that uses ZK.

## Problem

Suppose you want a name registry.

You want this to be:
* resistant to any coercion
* leaving no trace who owns a name

Because the users intend to use it for very critical things that they like privacy for.
Say naming their wallet address e.g. anon42's wallet address -> 0x696969696969.

Getting a wrong wallet address means, you pay a bad person instead of anon42.
Revealing who owns the name reveals information who might own the wallet.
Both are unacceptable to users.

The users might also want to use it for software releases e.g. declaring that this is a URL for Darkfi's v1.0.

We see there can be backdoor in many solutions. So they don't work for mission critical things.

1. If you run a database on a "cloud", the provider has physical access to the machine.
1. Domain owners can change what the domain name resolves to.
1. PKI is backdoored and there is man in the middle attack if you don't use https.

## Solution: Darkmap

An immutable name registry deployed on Darkfi.

The two features: 
* names can be immutable
* there is no trace who owns the name

### API: Get

From an end user perspective, they provide a dpath (i.e. a name) and get a value back.

```
provide: darkrenaissance::darkfi::v0_4_1
get:     0766e910aae7af482885d0a5b05ccb61ae7c1af4 (which is the commit for Darkfi v0.4.1, https://github.com/darkrenaissance/darkfi/commit/0766e910aae7af482885d0a5b05ccb61ae7c1af4)
```

### Syntax: Dpath

```
  Colon means the key is locked to particular value.
  For example, the key v0_4_1 is locked to 0766e910aae7af482885d0a5b05ccb61ae7c1af4
  in the name registry that darkrenaissance:darkfi points to.
  Helpful to that a tag always means the same commit.
	              v
darkrenaissance:darkfi:v0_4_1


  Dot means the key is not locked to a value. 
  It can be locked to a value later or be changed to a different value.
  For example, master (HEAD) currently maps to 85c53aa7b086652ed6d2428bf748f841485ee0e2,
  Helpful that master (HEAD) can change.
	              v
darkrenaissance:darkfi.master
```

## Implementation

> Note: This book assumes basic familiarity with contracts and blockchain. 
> It is good if you are familiar with Rust.
> But you would still be able to follow along even if you aren't, by inferring from the context.

```
# Repository
git clone https://github.com/darkrenaissance/darkmap
```

### Tool 1: wasm contract

In Darkfi, a contract is deployed as a wasm module. 
Rust has one of the best wasm support, so Darkmap is implemented in Rust.
In theory, any language that compiles to wasm can be used make a contract e.g. Zig.

### Tool 2: zk proofs

You can make a ZK scheme where:
* a user computes a ZK proof locally and submit along the transaction.
* the contract grants access to change a key-value mapping that user **owns** when received a valid proof.

## wasm contract: `entrypoint.rs`

There is a macro defining 4 entrypoints the contract runtime calls.
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
