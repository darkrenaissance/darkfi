# Start Here

## Directory Structure

DarkFi loosely follows the standardized Unix directory structure.

* All bundled applications are contained in `bin/` subdirectory.
* Random scripts and helpers such as build artifacts, node deployment
  or syntax highlighting is in `contrib/`.
* Documentation is in `doc/`.
* Example codes are in `example/`.
* Script utilities are in `script/`.
  See also the large `script/research/` subdir.
* All core library code is contained in `src/`.
  See [Architecture Overview](arch/overview.md) for a detailed
  description.
    * The `src/sdk/` crate is used by WASM contracts and core code.
      It contains essential primitives and code shared between them.
    * `src/serial/` is a crate containing the binary serialization code.
    * `src/contract/` contains our native bundled contracts. It's worth
      looking at these to familiarize yourself with what contracts on
      DarkFi are like.

## Using DarkFi

Refer to the main [README](../index.html) file for instructions on how
to install Rust and necessary dependencies.

Then proceed to the [Running a Node](testnet/node.md) guide.

## Join the Community

Although we have a Telegram, we don't believe in centralized
proprietary apps, and our core community organizes through our own
fully anonymous p2p chat system which has support for Tor and i2p.

Every Monday at 14:00 UTC (DST) or 15:00 UTC (ST) in #dev we have our
main project meeting.

See the guide on [darkirc](misc/darkirc/darkirc.md) for instructions
on joining the chat.

## Contributing as a Dev

Check out the [contributor's guide](dev/contrib/contrib.md) for where
to find tasks and submitting patches to the project.

If you're not a dev but wish to learn then take a look at the
[agorism hackers study guide](dev/learn.md).

Lastly familiarize yourself with the
[project architecture](arch/arch.md). The book also contains a
cryptography section with a helpful
[ZK explainer](crypto/zk_explainer.md).

DarkFi also has a [project spec](spec/crypto-schemes.md) and
a [DEP](dep/0001.md) (DarkFi Enhancement Proposals) system.

## Detailed Overview

Source code is under `src/` subdirectory. Main interesting modules are:

* `net/` is our own p2p network. There are sessions such as incoming or
  outgoing that have channels (connections). Protocols are attached to
  channels depending on the session. The p2p network is also
  multi-transport with support for TCP (+TLS), Tor and i2p. So you can
  access the p2p fully anonymously (network level privacy).
* `event_graph/` which is a DAG sync protocol used for ensuring
  eventual consistency of data, such as with chat systems (you don't
  drop any messages).
* `runtime/` is the WASM smart contract engine. We separate computation
  into several stages which is checks-effects-interactions paradigm in
  solidity but enforced in the smart contract explicitly. For example
  in the `exec()` phase, you can only read, whereas writes must occur
  in the `apply(update)` phase.
* `blockchain/` and `validator/` is the blockchain and consensus algos.
* `zk/` is the ZK VM, which simply loads bytecode which is used to
  build the circuits. It's a very simple model rather than the TinyRAM
  computation models. We opted for this because we prefer simplicity in
  systems design.
* `sdk/` contains a crypto SDK usable in smart contracts and
  applications. There are also Python bindings here, useful for making
  utilities or small apps.
* `serial/` contains our own serialization because we don't trust Rust
  serialization libs like serde. We also have async serialization and
  deserialization which is good for network code.
* `tx/` is the tx we use. Note signatures are not in the calldata as
  having this outside it allows more efficient verification (since you
  can do it in parallel and so on).
    * All DarkFi calls are precomputed ahead of time which is needed
      for ZK. Normally in ETH or other smart contract chains, the
      calldata is calculated where the function is invoked. Whereas in
      DarkFi the entire callgraph and calldata is bundled since ZK
      proofs must be computed ahead of time. This also improves things
      like static analysis and security (limiting call depth is easy
      to check before verification).
    * Verifying sigs or call depth ahead of time helps make the chain
      more attack resistant.
* `contract/` contains our native smart contracts. Namely:
    * `money`, which is multi-asset anonymous transfers, anonymous
      swaps and token issuance. The token issuance is programmatic.
      When creating a token, it commits to a smart contract which
      specifies how the token is allowed to be issued.
    * `deploy` for deploying smart contracts.
    * `dao`, which is a fully anonymous DAO. All the DAOs on chain are
      anonymous, including the amounts and activity of the treasury.
      All participants are anonymous, proposals are anonymous and votes
      are anonymous including the token weighted vote amount, and user
      identity. You cannot see who is in the DAO.

NOTE: We try to minimize external dependencies in our code as much as
possible. We even try to limit dependencies within submodules.

Inside `bin/` contains utilities and applications:

* `darkfid/` is the main daemon and `drk/` is the wallet.
* `dnet/` is a viewer to see the p2p traffic of nodes, and `deg/` is a
  viewer for the event graph data. We use these as debugging and
  monitoring tools.
* `dhtd/` is a distributed hash table, like IPFS, for transferring
  static data and large files around. Currently just a prototype but
  we'll use this later for images in the chat or other static content
  like seller pages on the marketplace.
* `tau/` is an anon p2p task manager which we use. We don't use Github
  issues, and seek to minimize our dependence on centralized services.
  Eventually we want to be fully p2p and attack resistant.
* `darkirc/` is our main community chat. It uses [RLN](crypto/rln.md);
  you stake money and if you post twice in an epoch then you get
  slashed which prevents spam. There is a free tier. It uses the
  `event_graph` for synchronizing the history. You can attach any IRC
  frontend to use it.
* `zkas/` is our ZK compiler.
* `zkrunner/` contains our ZK debugger (run `zkrunner` with `--trace`),
  and `zkrender` which renders a graphic of the circuit layout.
* `lilith/` is a universal seed node. Eventually we will add swarming
  support to our p2p network which is an easy addon.

Lastly worth taking a look is `script/research/` and
`script/research/zk/` which contains impls of most major ZK algos.
`bench/` contains benchmarks. `script/escrow.sage` is a utility for
doing escrow. We'll integrate it in the wallet later.

Our design philosophy and simplicity oriented approach to systemd dev:

* [Suckless Philosophy: software that sucks less](https://suckless.org/philosophy/)
* [How to Design Perfect (Software) Products  by Pieter Hintjens](http://hintjens.com/blog:19/noredirect/true)

Recent crypto code audit: [ZK Security DarkFi Code Audit](https://dark.fi/zksecurity-audit-q124.pdf)

Useful link on [our ZK toolchain](zkas/writing-zk-proofs.md)

For proof files, see `proof/` and `src/contract/*/proof/` subdirs.
