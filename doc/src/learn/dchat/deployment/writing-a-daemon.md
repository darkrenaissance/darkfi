# Writing a daemon

DarkFi consists of many seperate daemons communicating with each other. To
run the p2p network, we'll need to implement our own daemon.  So we'll
start building `dchat` by creating a daemon that we call `dchatd`.

To do this, we'll make use of a DarkFi macro called
[async_daemonize](https://codeberg.org/darkrenaissance/darkfi/src/branch/master/src/util/cli.rs).

`async_daemonize`is the standard way of daemonizing darkfi binaries. It
implements TOML config file configuration, argument parsing and a
multithreaded async executor that can be passed into the given function.

We use `async_daemonize` as follows:

```rust
use darkfi::{async_daemonize, cli_desc, Result};
use smol::stream::StreamExt;
use structopt_toml::{serde::Deserialize, structopt::StructOpt, StructOptToml};

const CONFIG_FILE: &str = "dchatd_config.toml";
const CONFIG_FILE_CONTENTS: &str = include_str!("../dchatd_config.toml");

#[derive(Clone, Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
#[structopt(name = "daemond", about = cli_desc!())]
struct Args {
    #[structopt(short, long)]
    /// Configuration file to use
    config: Option<String>,

    #[structopt(short, long)]
    /// Set log file to ouput into
    log: Option<String>,

    #[structopt(short, parse(from_occurrences))]
    /// Increase verbosity (-vvv supported)
    verbose: u8,
}

async_daemonize!(realmain);
async fn realmain(args: Args, ex: Arc<smol::Executor<'static>>) -> Result<()> {
    println!("Hello, world!");
    Ok(())
}
```

Behind the scenes, `async_daemonize` uses `structopt` and `structopt_toml`
crates to build command line arguments as a struct called `Args`. It spins
up a async executor using parallel threads, and implements signal handling
to properly terminate the daemon on receipt of a stop signal.

`async_daemonize` allow us to spawn the config data we specify at
`CONFIG_FILE_CONTENTS` into a directory either specified using the
command-line flag `--config`, or in the default darkfi config directory.

`async_daemonize` also implements logging that will output
different levels of debug info to the terminal, or to both the terminal
and a log file if a log file is specified.
