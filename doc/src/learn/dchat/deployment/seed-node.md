# The seed node

Let's try building `dchatd` at this point and running it. Assuming
`dchatd` is located in the `example/dchat` directory, we build it from
the `darkfi` root directory using the following command:

```bash
cargo build --all-features --package dchatd
```

On first run, it will create a config file from the defaults we specified
earlier. Run it as follows:

```bash
./dchatd
```
It should output the following:

```
[WARN] [P2P] Failure contacting seed #0 [tcp://127.0.0.1:50515]: IO error: connection refused
[WARN] [P2P] Seed #0 connection failed: IO error: connection refused
[ERROR] [P2P] Network reseed failed: Failed to reach any seeds
```

That's because there is no seed node online for our node to connect to. A
seed node is used when connecting to the network: it is a special kind
of inbound node that gets connected to, sends over a list of addresses
and disconnects again.  This behavior is defined in the `ProtocolSeed`.

Everytime we start an `OutboundSession`, we attempt to connect to a seed
using a `SeedSyncSession`.  If the `SeedSyncSession` fails we cannot
establish any outbound connections. Let's remedy that.

`darkfi` provides a standard seed node called `lilith` that can act as
the seed for many different protocols at the same time.

Just like any p2p daemon, a seed node defines its networks settings
from a config file, using the network type `Settings`. `lilith` allows
for multiple networks to be configured in its config file. Crucially,
each network must specify an `acccept_addr` which nodes on the network
can connect to.
