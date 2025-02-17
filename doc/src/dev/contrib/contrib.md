# Contributing

## How to get started

Join the dev chat, and attend a dev meeting. Make yourself known. Express an
interest in which areas of the code you want to work on. Get mentored by
existing members of the community.

Every monday 15:00 UTC, there is our main dev meeting on
[our chat](../../misc/darkirc/darkirc.md).
Feel free to join and discuss with other darkfi devs.

In general, the best way to get started is to explore the codebase thoroughly and
identify issues and areas of improvement.

Contribute according to your own interests, skills, and topics in which you would
like to become more knowledgable. Take initiative. Other darkfi devs can help you
as mentors: see [the Methodology section of the Study Guide](../learn.md#methodology).

Few people are able be an expert in all domains. Choose a topic and specialize.
Example specializations are described [here](../learn.md#branches).
Don't make the mistake that you must become an expert in all areas before getting started.
It's best to just jump in.

## Finding specific tasks

Check the tau task manager. There are a ton of tasks on there.

Tasks are usually noted in-line using code comments. All of these tasks should be resolved
and can be considered a priority.

To find them, run the following command:
```
$ git grep -E 'TODO|FIXME'
```

## Areas of work

There are several areas of work that are either undergoing maintenance 
or need to be maintained:

* **Documentation:** general documentation and code docs (cargo doc). This is a very 
  important work for example [overview](../../arch/overview.md) 
  page is out of date.
    * We need a tutorial on writing smart contracts. The tutorial could show
      how to make an anon ZK credential for a service like a forum.
    * Continuing on, it could show how to use the p2p network or event graph
      to build an anonymous service like a forum.
* **TODO** and **FIXME** are throughout the codebase. Find your favourite one and begin hacking.
* **Tooling:** Creating new tools or improving existing ones.
    * Improve the ZK tooling. For example tools to work with txs, smart contracts and ZK proofs.
    * Also document zkrunner and other tools.
* **Tests:** Throughout the project there are either broken or commented out unit tests, they need to be fixed.
* **DHT:** Currently this is broken and needs fixing.
* Harder **crypto** tasks:
    * Money::transfer() contract viewing keys

## Fuzz testing

Fuzz testing is a method to find important bugs in software. It becomes more 
powerful as more computing power is allocated to it. 

You can help to test DarkFi by running our fuzz tests on your machine. No
specialized hardware is required. 

As fuzz testing benefits from additional CPU power, a good method for running
the fuzzer is to let it run overnight or when you are otherwise not using
your device.

### Set-up
After running the normal commands to set-up DarkFi as described in the README, run the following commands.

```
# Install cargo fuzz
$ cargo install cargo-fuzz
```

Run the following from the DarkFi repo folder:

```
$ cd fuzz/
$ cargo fuzz list
```

This will list the available fuzzing targets. Choose one and run it with:

### Run
```
# format: cargo fuzz run TARGET
# e.g. if `serial` is your target:
$ cargo fuzz run --all-features -s none --jobs $(nproc) serial 
```

This process will run infinitely until a crash occurs or until it is cancelled by the user.

If you are able to trigger a crash, get in touch with the DarkFi team via irc.

Further information on fuzzing in DarkFi is available [here](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/fuzz/README.md).

## Troubleshooting

The `master` branch is considered bleeding-edge so stability issues can occur. If you
encounter issues, try the steps below. It is a good idea to revisit these steps
periodically as things change. For example, even if you have already installed all
dependencies, new ones may have been recently added and this could break your
development environment.

* Clear out artifacts and get a fresh build environment: 

```sh
# Get to the latest commit
$ git pull origin master
# Clean build artifacts
$ make distclean
```

* Remove `Cargo.lock`. This will cause Rust to re-evaluate dependencies and could help
if there is a version mismatch.

* Ensure all dependencies are installed. Check the README.md and/or run:

```
$ sh contrib/dependency_setup.sh
```

* Ensure that you are using the nightly toolchain and are building for `wasm32-unknown-unknown`.
Check `README.md` for instructions.

* When running a `cargo` command, use the flag `--all-features`.

## Security Disclosure

Join our DarkIRC chat and ask to speak with the core team.

Usually the best time would be our weekly Monday meetings at 15:00 UTC.

If it's sensitive and time critical, then we will get in touch over DM,
and we will post a message on dark.fi to confirm our identity once we're in
contact over DM.

We haven't yet clarified our bug bounty program (stay tuned), but for legit bug
reports we will pay out fairly.

