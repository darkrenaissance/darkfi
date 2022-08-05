# Settings

On production-ready software, you would usually configure your node
using a config file or command line inputs. On `dchat` we are keeping
things ultra simple. We pass a command line flag that is either `a` or
`b`. If we pass `a` we will initialize an inbound node. If we pass `b`
we will initialize an outbound node.

Here's how that works. We define two methods called `alice()` and
`bob()`. `alice()` returns the `Settings` that will create an inbound
node. bob() return the `Settings` for an outbound node.

We also implement logging that outputs to `/tmp/alice.log` and `/tmp/bob.log`
so we can access the debug output of our nodes. We store this info in a
log file because we don't want it interfering with our terminal UI when
we eventually build it.

This is a function that returns the settings to create Alice, an
inbound node:

```rust
{{#include ../../../../../example/dchat/src/main.rs:121:141}}
```

This is a function that returns the settings to create Bob, an
outbound node:

```rust
{{#include ../../../../../example/dchat/src/main.rs:143:161}}
```

Both outbound and inbound nodes specify a seed address to connect to. The
inbound node also specifies an external address and an inbound address:
this is where it will receive connections. The outbound node specifies
the number of outbound connection slots, which is the number of outbound
connections the node will try to make.

These are the only settings we need to think about. For the rest, we
use the network defaults.

