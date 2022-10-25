# Settings

To create an inbound and outbound node, we
will need to configure them using `net` type called
[Settings](https://github.com/darkrenaissance/darkfi/blob/master/src/net/settings.rs).
This type consists of several settings that allow you to configure nodes
in different ways.

You would usually configure `Settings` using a config file or command
line inputs. On dchat we are keeping things ultra simple. We pass a
command line flag that is either `a` or `b`. If we pass `a` we will
initialize the `Settings` for an inbound node. If we pass `b` we will
initialize an outbound node.

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
       external_addr: Some(ext_addr),
       seeds: vec![seed],
       ..Default::default()
   };

   Ok(settings)
}
```

This is a function that returns the settings to create Bob, an
outbound node:

```rust
fn bob() -> Result<Settings> {
   let log_level = simplelog::LevelFilter::Debug;
   let log_config = simplelog::Config::default();

   let log_path = "/tmp/bob.log";
   let file = File::create(log_path).unwrap();
   WriteLogger::init(log_level, log_config, file)?;

   let seed = Url::parse("tcp://127.0.0.1:55555").unwrap();

   let settings = Settings {
       inbound: None,
       outbound_connections: 5,
       seeds: vec![seed],
       ..Default::default()
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

