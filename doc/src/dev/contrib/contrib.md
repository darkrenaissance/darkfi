# Contributing

## How to get started

1. Join the dev chat, and attend a dev meeting.
2. See the areas of work below. Good areas to get started are with
   tooling, Python bindings, p2p apps like the DHT.

Every monday 16:00 CET, there is our main dev meeting on
[our chat](https://darkrenaissance.github.io/darkfi/misc/darkirc/darkirc.html).
Feel free to join and discuss with other darkfi devs.

In general, the best way to get started is to explore the codebase thoroughly and
identify issues and areas of improvement.

Contribute according to your own interests, skills, and topics in which you would
like to become more knowledgable. Take initiative. Other darkfi devs can help you
as mentors: see [the Methodology section of the Study Guide](https://darkrenaissance.github.io/darkfi/dev/learn.html#methodology).

Few people are able be an expert in all domains. Choose a topic and specialize.
Example specializations are described [here](https://darkrenaissance.github.io/darkfi/dev/learn.html#branches).
Don't make the mistake that you must become an expert in all areas before getting started.
It's best to just jump in.

## Finding specific tasks

Tasks are usually noted in-line using code comments. All of these tasks should be resolved
and can be considered a priority.

To find them, run the following command:
```
$ git grep -E 'TODO|FIXME'
```

## Employment

We are only looking for devs right now. If you're not a dev, see the
[learn section](dev/learn.md). We offer mentoring. Anybody can become a dev.
It's not that hard, you just need focus and dedication.

To be hired as a dev, you must make commits to the repo, preferably
more than minor cosmetic changes. It also is useful to have online repositories
containing your work. We don't care about degrees or qualifications -
many of the best coders don't have any.

Secondly you need to get on [our online chat](misc/darkirc/darkirc.md) and
make yourself known. We are not spending time on social media or proprietary
chats like Telegram because we're very busy.

We value people who have initiative. We value this so highly in fact that
even if someone is less skilled but shows the ability to learn, we will welcome
them and give them everything they need to prosper. Our philosophy is that
of training leaders rather than hiring workers. Our team is self-led. We don't
have any managers or busybody people. We discuss the problems amongst ourselves
and everybody works autonomously on tasks. We don't keep people around who
need a manager looking over their shoulder. The work and tasks should be
obvious, but to help you along below you will find lists of tasks to get started
on.

## Areas of work

There are several areas of work that are either undergoing maintenance 
or need to be maintained:

* **Documentation:** general documentation and code docs (cargo doc). This is a very 
  important work for example [overview](https://darkrenaissance.github.io/darkfi/arch/overview.html) 
  page is out of date.
    * We need a tutorial on writing smart contracts. The tutorial could show
      how to make an anon ZK credential for a service like a forum.
    * Continuing on, it could show how to use the p2p network or event graph
      to build an anonymous service like a forum.
* **TODO** and **FIXME** are throughout the codebase. Find your favourite one and begin hacking.
    * DarkIRC encrypted DMs to nonexistant users should not be allowed.
    * Currently closing DarkIRC with ctrl-c stalls in `p2p.stop()`. This should be fixed.
    * Add `log = path` and `log_level = debug` config setting to DarkIRC
    * StoppableTask should panic when we call stop() on a task that has not been started.
* **Tooling:** Creating new tools or improving existing ones.
    * Improve the ZK tooling. For example tools to work with txs, smart contracts and ZK proofs.
    * Also document zkrunner and other tools.
* **Tests:** Throughout the project there are either broken or commented out unit tests, they need to be fixed.
* **Cleanup:** General code cleanup. for example flattening headers and improving things like in 
  [this commit](https://codeberg.org/darkrenaissance/darkfi/commit/9cd9c3113eed1b5f0bcad2ee449ef926d0908d55).
* **Python bindings:** Help ensure wider coverage and cleanup the Python bindings in `src/sdk/python/`.
    * The event graph could have Python bindings but involves some tricky part integrating Python and Rust async.
    * Bindings for txs, calls and so on. Make a tool in Python for working with various contract params.
* **Events System:** See the
  [event graph](https://darkrenaissance.github.io/darkfi/misc/event_graph/event_graph.html) system.
  We need extra review of the code and improvement of the design. This is a good submodule to begin working on.
* **DHT:** Currently this is broken and needs fixing.
* **p2p Network:** this is a good place to start reviewing the code and suggesting improvements.
  For example maintaining network resiliency. You can also look at apps like darkirc, and the event graph subsystem,
  and see how to make them more reliable. See also the task manager tau.
    * Implement resource manager. See its implementation in libp2p for inspiration.
* Harder **crypto** tasks:
    * Money viewing keys
* Eth-DarkFi bridge or atomic swaps. Atomic swaps is probably better since it's trustless and p2p.

## Mainnet tasks

_Tasks are in no particular order. Use common sense._

1. Finish `darkfid` with PoW and research and implement XMR merge mining
2. Make `darkfi-mmproxy` stable and implement what is needed for DarkFi x Monero merge mining
3. Finish dnetview
4. Make `eventgraph` stable and implement proper unit and integration tests
  * Unit tests should test pieces of the eventgraph code
  * Integration tests should simulate a P2P network and ensure deterministic state after a simulated run
  * Update https://darkrenaissance.github.io/darkfi/misc/event_graph/event_graph.html
    and make it the specification for the `eventgraph` implementation.
5. Rework `drk` (the wallet CLI) to work standalone and make it work with the new `darkfid`
6. Make `tau` stable
7. Make `darkirc` stable
8. Make `lilith` stable, there is currently some bug that causes connection refusals
9. Implement transaction fees logic
10. Implement contracts deployment logic
11. Revisit **all** the code inside `src/runtime/` and make sure it's safe
12. ~~Implement verifiable encryption for `DAO` payments~~
13. ~~`DAO` should be able to perform arbitrary contract calls, it should act as a voted multisig~~
14. Implement cross-chain atomic swaps (XMR, ETH, anything applicable)
15. ~~Rework the connection algo for p2p to use black list, grey and white list~~
  * ~~https://eprint.iacr.org/2019/411.pdf (Section 2.2)~~
  * ~~See also [P2P Network: Common Mitigations](arch/p2p-network.md#common-mitigations)~~
16. Create a P2P stack test harness in order to be able to easily simulate network
    behaviour
  * Possibly we can create a dummy p2p which can simulate network connections and routing traffic.
    We can use this to model network behaviour.
17. Implement address/secretkey differentiation
  * See [WIF](https://en.bitcoin.it/wiki/Wallet_import_format)
18. ~~Fix bugs and issues in the DAO implementation~~
19. Perform thorough review of all contracts and their functionalities
20. Randomize outputs in `Money::*`, and potentially elsewhere where applicable
  * This is so the change output isn't always in the same predictable place, and makes identifying
    which output is the change impossible.
21. Document contracts in the manner of https://darkrenaissance.github.io/darkfi/arch/consensus/stake.html
22. Serial used in money coins
  * One solution is: don't accept coins with existing serial in drk.
  * We should construct a scheme to derive the serial, evaluate how simple changing the crypto is.
  * Malicious users could send you a coin which is unspendable. A poorly implemented wallet would
    accept such a coin, and if spent then you would be unable to spend the other coin sharing the same
    serial in your wallet.
23. Separate mining logic from darkfid into a new program and communicate over RPC
24. Python utility tool (swiss army knife) for working with txs, contract calls and params.
25. Python event viewer to inspect and debug the event graph.
26. Fix `protocol_address` for anonymity. There is a loop sending self addr constantly. We should
    have this mixed with a bunch of random addrs to avoid leaking our own addr.
27. Add support for colorizing zkas code samples in darkfi book (see arch/dao page)
28. Tutorial creating a ZK credentials scheme.
29. resource manager for p2p (DoS protection, disconnect bad nodes)
30. apply DEP 0001
31. fix channel `main_receive_loop()` to use `Weak`
32. configurable MAGIC_BYTES for net code
33. configurable fields for version messages
34. make `PeerDiscovery` in `outbound_session.rs` a trait object which is
    configurable in P2p, but by default is set to `PeerSeedDiscovery`.


|  Task #  |  Assignee  |
|----------|------------|
| **1.**   | `upgrayedd`|
| **2.**   | `brawndo`  |
| **3.**   | `lain`     |
| **4.**   | `upgrayedd`|
| **5.**   | `upgrayedd`|
| **6.**   | `dasman`   |
| **7.**   | `dasman`   |
| **8.**   | `brawndo`  |
| **9.**   | `brawndo`  |
| **10.**  | `brawndo`  |
| **11.**  |            |
| **12.**  | `B1-66ER`  |
| **13.**  | `B1-66ER`  |
| **14.**  |            |
| **15.**  | `lain`     |
| **16.**  | `lain`     |
| **17.**  |            |
| **18.**  | `B1-66ER`  |
| **19.**  | `B1-66ER`  |
| **20.**  |            |
| **21.**  | `B1-66ER`  |
| **22.**  | `B1-66ER`  |
| **23.**  | `upgrayedd`|
| **24.**  |            |
| **25.**  | `lain`     |

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

Usually the best time would be our weekly Monday meetings at 16:00 CET.

If it's sensitive and time critical, then we will get in touch over DM,
and we will post a message on dark.fi to confirm our identity once we're in
contact over DM.

We haven't yet clarified our bug bounty program (stay tuned), but for legit bug
reports we will pay out fairly.

