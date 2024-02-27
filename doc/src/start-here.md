# Start Here

## Directory Structure

DarkFi broadly follows the standardized unix directory structure.

* All bundled applications are contained in `bin/` subdirectory.
* Random scripts and helpers such as build artifacts, node deployment
  or syntax highlighting is in `contrib/`.
* Documentation is in `doc/`.
* Example codes are in `example/`.
* Script utilities are in `script/`. See also the large `script/research/`
  subdir.
* All core code is contained in `src/`.
  See [Architecture Overview](arch/overview.md) for a detailed description.
    * The `src/sdk/` crate is used by WASM contracts and core code.
      It contains essential primitives and code shared between them.
    * `src/serial/` is a crate containing the binary serialization code,
      which is the same as used by Bitcoin.
    * `src/contract/` contains our native bundled contracts. It's worth
      looking at these to familiarize yourself with what contracts on DarkFi
      are like.

## Using DarkFi

Refer to the main
[README](https://github.com/darkrenaissance/darkfi/blob/master/README.md)
file for instructions on how to install Rust and necessary deps.

Then proceed to the [Running a Node](testnet/node.md) guide.

## Join the Community

Although we have a Telegram, we don't believe in centralized proprietary apps,
and our core community organizes through our own fully anonymous p2p chat system
which has support for Nym and Tor.

Every Monday at 16:00 CET in #dev we have our main project meeting.

See the guide on [darkirc](misc/ircd/ircd.md) for instructions on joining.

## Contributing as a Dev

Check out the [contributor's guide](dev/contrib/contrib.md) for where to find
tasks and submitting patches to the project.

If you're not a dev but wish to learn then take a look at the
[agorism hackers study guide](dev/learn.md).

Lastly familiarize yourself with the [project architecture](arch/arch.md).
The book also contains a cryptography section with a helpful
[ZK explainer](crypto/zk_explainer.md).

DarkFi also has a [project spec](spec/crypto-schemes.md) and
a [DEP](dep/0001.md) (Drk Enhancement Proposals) system.

