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
# Build the zkas compiler
cd $HOME && git clone https://github.com/darkrenaissance/darkfi darkfi-master
cd darkfi && make zkas
PATH="$PATH:$PWD"

cd $HOME && git clone https://github.com/darkrenaissance/darkmap
cd darkmap && make
```

## Tool 1: zkas, zkvm

We want a way for someone to control an account. You could use public key 
crytography. But in here, we will use zk to accomplish the same thing.

In Darkfi, circuits are programmed in `zkas` (ZK ASsembly) and later run in zkvm to generate proofs.

There is one circuit that Darkmap uses, which is the set circuit for gating the `set` function. Let's see what it does and
start reading `<darkmap>/proof/set_v1.zk`.

## Tool 2: zkrunner, darkfi-sdk-py

We mentioned zkas circuits are "run inside" zkvm. How?

There is a developer facing cli `zkrunner`. The cli allows you to interact with zkvm in Python.

Let's see how to run the `set_v1.zk` by reading `<darkfi>/bin/zkrunner/README.md`.

## Tool 3: wasm contract

In Darkfi, a contract is deployed as a wasm module. 
Rust has one of the best wasm support, so Darkmap is implemented in Rust.
In theory, any language that compiles to wasm can be used make a contract e.g. Zig.

Let's read `<darkmap>/src/entrypoints.rs`.

## Notes

* Where are the states stored?
	* There is no memory, there is calldata
* What are the runtime functions you can call?
	* db_init
	* db_lookup
	* zkas_db_set
* What are the tools?
	* zkas
        * zkvm
        	* the runtime imports
	* wasm
	* client
		* transaction builder
	* test facility

