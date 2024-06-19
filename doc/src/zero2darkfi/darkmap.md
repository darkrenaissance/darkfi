We are going to walk through a simple contract that uses ZK.

## Problem

Suppose you want to build a name registry.

You want this to be:
* resistant to any coercion
* leaving no trace who owns a name

Because the users intend to use it for critical things that they like privacy for.
Say naming their wallet address e.g. anon42's wallet address -> 0x696969696969.

Getting a wrong wallet address means, you pay a bad person instead of anon42.
Revealing who owns the name reveals information who might own the wallet.
Both are unacceptable to your users.

Upon examination we see backdoor in many solutions.

1. If you run a database on a "cloud", the provider has physical access to the machine.
1. Domain owners can change what the domain name resolves to.

## Solution: Darkmap

An immutable name registry deployed on Darkfi.

* names can be immutable, not even the name registry owner can change the name
* there is no trace who owns the name

### API: Get

From an end user perspective, they provide a dpath and get a value back.

```
provide: darkrenaissance::darkfi::v0_4_1
get:     0766e910aae7af482885d0a5b05ccb61ae7c1af4 (which is the commit for Darkfi v0.4.1, https://codeberg.org/darkrenaissance/darkfi/commit/0766e910aae7af482885d0a5b05ccb61ae7c1af4)
```

### Syntax

```
  Colon means the key is locked to particular value.
  For example, the key v0_4_1 is locked to 0766e910aae7af482885d0a5b05ccb61ae7c1af4
  in the name registry that darkrenaissance:darkfi points to.
  Helpful to that a tag always means the same commit.
                  v
darkrenaissance:darkfi:v0_4_1
   ^               ^ 
   |                \
   |                 \
   |                  \
top level registry    sub registry


  Dot means the key is not locked to a value. 
  It can be locked to a value later or be changed to a different value.
  For example, master (HEAD) currently maps to 85c53aa7b086652ed6d2428bf748f841485ee0e2,
  Helpful that master (HEAD) can change.
                  v
darkrenaissance:darkfi.master


All parts except the last resolve to a name registry.
* darkrenaissance is a top level registry, it resolves to an account controlled by an anonymous owner
* darkfi is a sub registry, for example darkrenaissance:darkfi resolves to an account
* there can be multiple paths to a name registry, for example, dm:darkfi can resolve to the same account as above
```

## Implementation

```
# Let's begin by building the zkas compiler
git clone https://github.com/darkrenaissance/darkfi
cd darkfi && make zkas
PATH="$PATH:$PWD"

# Pull down the darkmap contract for our learning
cd ../ && git clone https://github.com/darkrenaissance/darkmap
```

## Tool 1: `ZKAS`, `ZKVM`

We want a way for someone to control an account and account to control one name registry. 
You could use public key crytography.
But in here, we will use ZK to accomplish the same thing for our learning.

In Darkfi, circuits are programmed in `ZKAS` (ZK Assembly) and later run in `ZKVM` for generating proofs.

There is one circuit that Darkmap uses, which is the `set` circuit for gating the `set` function.

Let's see what it does and start reading `<darkmap>/proof/set_v1.zk`.

### `zkrunner`, `darkfi-sdk-py`

We mentioned ZKAS circuits are "run inside" ZKVM. How?

There is a developer facing CLI zkrunner. The CLI allows you to interact with ZKVM in Python.

Let's see how to run the set_v1.zk by reading `<darkfi>/bin/zkrunner/README.md`.

### Outcome

Good job! Now you have you learned how to prove and run using a ZKAS circuit.

## Tool 2: WASM contract

In Darkfi, a contract is deployed as a wasm module. 
Rust has one of the best wasm support along with C and C++, so Darkmap is implemented in Rust.
In theory, any language that compiles to wasm can be used make a contract.

Let's learn about the contract by reading `<darkmap>/src/entrypoints.rs`.

## Deploying, testing and client

FIXME: perhaps more detailed explanation

## Deploying 

Currently, the infrastructure for deploying non-native contracts is being worked on. 
So Darkmap was tested by modifiying the darkfi validator to deploy it as native contract.

If you like to try it out, take a look at the [pull request draft](https://codeberg.org/darkrenaissance/darkfi/pulls/170/files#diff-1592d061816d5a4da17e089758e15df75ae1ab963b2288e6d84b8f29b06f7d4f).

In particular:
* `src/consensus/validator.rs`
* `sdk/src/crpyto/*`

## Testing and client implementation

For now, the best place to learn is learn from the darkmap pull request draft or `src/contract/money`.

## Notes

* Where are the states stored?
* What are the host-provided functions you can call from within the contract?
	* https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/runtime/import
* What are the tools?
	* zkas
        * zkvm
        * the host-provided imports
	* wasm
	* client
	* transaction builder
	* testing libraries and functions

