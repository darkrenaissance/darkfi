# Writing a daemon

DarkFi consists of many seperate daemons communicating with each other. To
run the p2p network, we'll need to implement our own daemon.  So we'll
start building `dchat` by configuring our main function into a daemon that
can run the p2p network.

```rust
{{#include ../../../../../example/dchat/src/main.rs::9}}

{{#include ../../../../../example/dchat/src/main.rs:23:24}}

{{#include ../../../../../example/dchat/src/main.rs:163:164}}
{{#include ../../../../../example/dchat/src/main.rs:176:179}}

{{#include ../../../../../example/dchat/src/main.rs:186:189}}
{{#include ../../../../../example/dchat/src/main.rs:191:197}}
```

We get the number of cpu cores using `num_cpus::get()` and spin up a
bunch of threads in parallel using `easy_parallel`. Right now it doesn't
do anything, but soon we'll run dchat inside this block.

**Note**: DarkFi includes a macro called `async_daemonize` that is used by
DarkFi binaries to minimize boilerplate in the repo.  To keep things
simple we will ignore this macro for the purpose of this tutorial.  But
check it out if you are curious: [util/cli.rs](https://github.com/darkrenaissance/darkfi/blob/master/src/util/cli.rs#L154).

