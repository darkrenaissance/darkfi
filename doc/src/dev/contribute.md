# Contributing

## How to get started

Every monday 16:00 CET, there is our main dev meeting on
[our chat](https://darkrenaissance.github.io/darkfi/misc/darkirc/darkirc.html).
Feel free to join and discuss with other darkfi devs.

In general, the best way to get started is to explore the codebase thoroughly and
identify issues and areas of improvement.

Contribute according to your own interests, skills, and topics in which you would
like to become more knowledgable. Take initiative. Other darkfi devs can help you
as mentors: see [the Methodology section of the Study Guide](https://darkrenaissance.github.io/darkfi/development/learn.html#methodology).

Few people are able be an expert in all domains. Choose a topic and specialize.
Example specializations are described [here](https://darkrenaissance.github.io/darkfi/development/learn.html#branches).
Don't make the mistake that you must become an expert in all areas before getting started.
It's best to just jump in.

## Finding specific tasks

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
  important work for example [overview](https://darkrenaissance.github.io/darkfi/architecture/overview.html) 
  page is out of date.
* **Tooling:** Such as the `drk` tool. right now 
  we're adding [DAO functionality](https://github.com/darkrenaissance/darkfi/blob/master/src/contract/dao/wallet.sql) 
  to it.
* **Tests:** Throughout the project there are either broken or commented out unit tests, they need to be fixed.
* **Cleanup:** General code cleanup. for example flattening headers and improving things like in 
  [this commit](https://github.com/darkrenaissance/darkfi/commit/9cd9c3113eed1b5f0bcad2ee449ef926d0908d55).
* **Python bindings:** Help ensure coverage and cleanup the Python bindings in `src/sdk/python/`.
* **Events System:** We need to fix IRCD, we will need to implement the 
  [events](https://darkrenaissance.github.io/darkfi/misc/event_graph/event_graph.html) system.
* **p2p Network:** this is a good place to start reviewing the code and suggesting improvements.
  For example maintaining network resiliency. You can also look at apps like darkirc, and the event graph subsystem,
  and see how to make them more reliable. See also the task manager tau.

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

Further information on fuzzing in DarkFi is available [here](https://github.com/darkrenaissance/darkfi/blob/master/fuzz/README.md).

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
