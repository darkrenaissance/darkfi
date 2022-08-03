# Inbound and Outbound nodes

To create an instance of the p2p network, we must configure our p2p
network settings into a type called net::Settings. These settings
determine whether our node will be an outbound, inbound, manual or
seed node.

Inbound, outbound and seed nodes perform different roles on the p2p
network. An inbound node receives connections. An outbound node makes
connections. A seed node is used when connecting to the network: it is
a special kind of inbound node that gets connected to, sends over a list
of addresses and disconnects again.

The behavior of the different
kinds of nodes is defined in what is called a
[Session](https://github.com/darkrenaissance/darkfi/blob/master/src/net/session/mod.rs#L93).
Session is a trait that outbound, inbound, manual and seed nodes all
implement. Session implementations expose methods such as stopping and
starting a channel, accepting connections (inbound nodes) or making
connections (outbound nodes).

On production-ready software, you would usually configure your node
using a config file or command line inputs. On dchat we are keeping
things ultra simple. We pass a command line flag that is either `a` or
`b`. If we pass `a` we will initialize an inbound node. If we pass `b`
we will initialize an outbound node.

Here's how that works. We define two methods called alice() and
bob(). alice() returns the Settings that will create an inbound
node. bob() return the Settings for an outbound node.

We also implement logging that outputs to /tmp/alice.log and /tmp/bob.log
so we can access the debug output of our nodes. We store this info in a
log file because we don't want it interfering with our terminal UI when
we eventually build it.

This is a function that returns the settings to create Alice, an
inbound node:

```
use simplelog::WriteLogger;
use std::fs::File;

use darkfi::{net::Settings, Result};
use url::Url;

fn alice() -> Result<Settings> {
    let log_level = simplelog::LevelFilter::Debug;
    let log_config = simplelog::Config::default();

    let log_path = "/tmp/alice.log";
    let file = File::create(log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;

    let seed = Url::parse("tcp://127.0.0.1:55555").unwrap();
    let inbound = Url::parse("tcp://127.0.0.1:55554").unwrap();
    let ext_addr = Url::parse("tcp://127.0.0.1:55554").unwrap();

    let settings = Settings {
        inbound: Some(inbound),
        outbound_connections: 0,
        manual_attempt_limit: 0,
        seed_query_timeout_seconds: 8,
        connect_timeout_seconds: 10,
        channel_handshake_seconds: 4,
        channel_heartbeat_seconds: 10,
        outbound_retry_seconds: 1200,
        external_addr: Some(ext_addr),
        peers: Vec::new(),
        seeds: vec![seed],
        node_id: String::new(),
    };

    Ok(settings)
}

```

This is a function that returns the settings to create Bob, an
outbound node:

```
fn bob() -> Result<Settings> {
    let log_level = simplelog::LevelFilter::Debug;
    let log_config = simplelog::Config::default();

    let log_path = "/tmp/bob.log";
    let file = File::create(log_path).unwrap();
    WriteLogger::init(log_level, log_config, file)?;
    let seed = Url::parse("tcp://127.0.0.1:55555").unwrap();
    let oc = 5;

    let settings = Settings {
        inbound: None,
        outbound_connections: oc,
        manual_attempt_limit: 0,
        seed_query_timeout_seconds: 8,
        connect_timeout_seconds: 10,
        channel_handshake_seconds: 4,
        channel_heartbeat_seconds: 10,
        outbound_retry_seconds: 1200,
        external_addr: None,
        peers: Vec::new(),
        seeds: vec![seed],
        node_id: String::new(),
    };

    Ok(settings)
}
```

Both outbound and inbound nodes specify a seed address to connect to. The
inbound node also specifies an external address and an inbound address:
this is where it will receive connections. The outbound node specifies
the number of outbound connection slots, which is the number of outbound
connections the node will try to make.

These are the only settings we need to think about. For the rest, we
use the network defaults.

