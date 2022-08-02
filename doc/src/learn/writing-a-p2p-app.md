# Writing a p2p app

This tutorial will teach you how to deploy an app on DarkFi's p2p network.

We will create a terminal-based p2p chat app called dchat that we run
in two different instances: an inbound and outbound node called Alice
and Bob. Alice takes a message from stdin and broadcasts it to the
p2p network. When Bob receives the message on on the p2p network it is
displayed his dchat UI.

Dchat will showcase some key concepts that you'll need to develop on
the p2p network, in particular:

* Understanding inbound, outbound and seed nodes.
* Writing and registering a custom protocol.
* Creating and subscribing to a custom message type.

The source code for this tutorial can be found at
[example/dchat](../../../example/dchat).

## Part 1: Deploying the p2p network
### Getting started

We'll create a new cargo directory and add DarkFi to our Cargo.toml,
like so:

```
[package]
name = "dchat"
version = "0.1.0"
edition = "2021"
description = "Demo chat to document darkfi net code"

[dependencies]
darkfi = {path = "../../", features = ["net"]}
```

Be sure to replace the path to DarkFi with the correct path for your
setup.

Once that's done we can access DarkFi's net methods inside of
dchat. We'll need a few more external libraries too, so add these
dependencies:

```
# Async
async-std = "1"
async-trait = "0.1.56"
async-executor = "1.4.1"
async-channel = "1.6.1"
easy-parallel = "3.2.0"
smol = "1.2.5"
num_cpus = "1.13.1"

# Misc
log = "0.4.17"
simplelog = "0.12.0"
url = "2.2.2"

# Encoding and parsing
serde = {version = "1.0.138", features = ["derive"]}
toml = "0.4.2"
```

### Writing a daemon

DarkFi consists of many seperate daemons communicating with each other. To
run the p2p network, we'll need to implement our own daemon.  So we'll
start building dchat by configuring our main function into a daemon that
can run the p2p network.

```
use async_executor::Executor;
use async_std::sync::Arc;
use easy_parallel::Parallel;

use std::fs::File;
use simplelog::WriteLogger;

use darkfi::Result;

#[async_std::main]
async fn main() -> Result<()> {
    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    //let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| {
            smol::future::block_on(ex.run(shutdown.recv()))
        })
        .finish(|| {
            smol::future::block_on(async move {
                // TODO
                // dchat.start(ex2).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
```

We get the number of cpu cores using num_cpus::get() and spin up a bunch
of threads in parallel using easy_parallel. For now it's commented out,
but soon we'll run dchat inside this block.

Note: DarkFi includes a macro called async_daemonize that is used by
DarkFi binaries to minimize boilerplate in the repo.  To keep things
simple we will ignore this macro for the purpose of this tutorial.  But
check it out if you are curious: [util/cli.rs](../../../src/util/cli.rs).

### Inbound and Outbound nodes

To create an instance of the p2p network, we must configure our p2p
network settings into a type called net::Settings. These settings
determine whether our node will be an outbound, inbound, manual or
seed node.

Inbound, outbound and seed nodes perform different roles on the p2p
network. An inbound node receives connections. An outbound node makes
connections. A seed node is used when connecting to the network: it is
a special kind of inbound node that gets connected to, sends over a list
of addresses and disconnects again.

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

### Error handling 

Before we continue, we need to quickly add some error handling to handle
the case where a user forgets to add the command-line flag. We'll use a
Box<dyn error::Error> to implement that. Because we are now defining our own
Result type, we will need to remove `use darkfi::Result` from main.rs.

```
use std::{error, fmt};

pub type Error = Box<dyn error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct MissingSpecifier;

impl fmt::Display for MissingSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "missing node specifier. you must specify either a or b")
    }
}

impl error::Error for MissingSpecifier {}
```

Finally we can read the flag from the command-line by adding the following lines to main():

```
let settings: Result<Settings> = match std::env::args().nth(1) {
    Some(id) => match id.as_str() {
        "a" => alice(),
        "b" => bob(),
        _ => Err(MissingSpecifier.into()),
    },
    None => Err(MissingSpecifier.into()),
};
```

### Creating the p2p network

Now that we have initialized the network settings we can create an
instance of the p2p network.

Add the following to main():

```
let p2p = net::P2p::new(settings?.into()).await;
```

### Running the p2p network

We will next create a Dchat struct that will store all the data required
by dchat. For now, it will just hold a pointer to the p2p network.

To accesss this we will need to add net to our imports, like so:

```
use darkfi::net;

struct Dchat {
    p2p: net::P2pPtr,
}

impl Dchat {
    fn new(p2p: net::P2pPtr) -> Self {
        Self { p2p }
    }
}
```

Now let's add a start() function to the Dchat implementation. start()
takes an executor and runs two p2p methods, p2p::start() and p2p::run().

```
async fn start(&self, ex: Arc<Executor<'_>>) -> Result<()> {

    self.p2p.clone().start(ex.clone()).await?;
    self.p2p.clone().run(ex.clone()).await?;

    Ok(())
}
```

Let's take a quick look at the underlying p2p methods we're using here.

This is [start()]("../../../src/net/p2p.rs"):

```
pub async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
    debug!(target: "net", "P2p::start() [BEGIN]");

    *self.state.lock().await = P2pState::Start;

    // Start seed session
    let seed = SeedSession::new(Arc::downgrade(&self));
    // This will block until all seed queries have finished
    seed.start(executor.clone()).await?;

    *self.state.lock().await = P2pState::Started;

    debug!(target: "net", "P2p::start() [END]");
    Ok(())
}
```

start() changes the P2pState to P2pState::Start and runs a [seed
session]("../../../src/net/session/seed_session.rs").

This loops through the seed addresses specified in our Settings and
tries to connect to them. The seed session either connects successfully,
fails with an error or times out.

If a seed node connects successfully, it runs a version exchange protocol,
stores the channel in the p2p list of channels, and disconnects, removing
the channel from the channel list.

### The seed node

Let's create an instance of dchat inside our main function and pass the
p2p network into it.  Then we'll add dchat::start() to our async loop
in the main function. 

```
#[async_std::main]
async fn main() -> Result<()> {
    let settings: Result<Settings> = match std::env::args().nth(1) {
        Some(id) => match id.as_str() {
            "a" => alice(),
            "b" => bob(),
            _ => Err(MissingSpecifier.into()),
        },
        None => Err(MissingSpecifier.into()),
    };

    let p2p = net::P2p::new(settings?.into()).await;
    let dchat = Dchat::new(p2p);

    let nthreads = num_cpus::get();
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex = Arc::new(Executor::new());
    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| {
            smol::future::block_on(ex.run(shutdown.recv()))
        })
        .finish(|| {
            smol::future::block_on(async move {
                dchat.start(ex2).await?;
                drop(signal);
                Ok(())
            })
        });

    result
}
```
Now try to run the program, don't forget to add a specifier `a` or `b`
to define the type of node.

It should output the following error: 

```
Error: NetworkOperationFailed
```

That's because there is no seed node online for our nodes to connect
to. Let's remedy that.

We have two options here. First, we could implement our own seed node.
Alternatively, DarkFi maintains a master seed node called seedd that
can act as the seed for many different protocols at the same time. To
be consistent with the rest of the code base, let's use seedd.

What this node does in the background is very simple. Just like any p2p
daemon, a seed node defines its networks settings into a type called
Settings and creates a new instance of the p2p network. It then runs
p2p::start() and p2p::run(). The difference is in the settings: a seed
node just specifies an inbound address which other nodes will connect to.

Crucially, this inbound address must match the seed address we specified
earlier in Alice and Bob's settings.

### Deploying a local network

Get ready to spin up a bunch of different terminals. We are going to
run 3 nodes: Alice and Bob and our seed node. To run the seed node,
go to the seedd directory and run it by passing dchat as an argument:

```
cargo run -- --dchat
```

Here's what the debug output should look like:

```
[DEBUG] (1) net: P2p::start() [BEGIN]
[DEBUG] (1) net: SeedSession::start() [START]
[WARN] Skipping seed sync process since no seeds are configured.
[DEBUG] (1) net: P2p::start() [END]
[DEBUG] (1) net: P2p::run() [BEGIN]
[INFO] Starting inbound session on tcp://127.0.0.1:55555
[DEBUG] (1) net: tcp transport: listening on 127.0.0.1:55555
[INFO] Starting 0 outbound connection slots.
```

Next we'll run Alice.

```
cargo run a
```

You can `cat` or `tail` the log file created in /tmp/. I recommend using
multitail for colored debug output, like so:

`multitail -c /tmp/alice.log`

Check out that debug output! Keep an eye out for this line:

```
[INFO] Connected seed #0 [tcp://127.0.0.1:55555]
```

That shows Alice has connected to the seed node. Here's some more
interesting output:

```
08:54:59 [DEBUG] (1) net: Attached ProtocolPing
08:54:59 [DEBUG] (1) net: Attached ProtocolSeed
08:54:59 [DEBUG] (1) net: ProtocolVersion::run() [START]
08:54:59 [DEBUG] (1) net: ProtocolVersion::exchange_versions() [START]
```

This raises an interesting question- what are these protocols? We'll deal
with that in more detail in a subsequent section. For now it's worth
noting that every node on the p2p network performs several protocols
when it connects to another node.

Keep Alice and the seed node running. Now let's run Bob.

```
cargo run b
```

And track his debug output:

```
multitail -c /tmp/bob.log
```

Success! All going well, Alice and Bob are now connected to each
other. We should be able to watch Ping and Pong messages being sent
across by tracking their debug output.

We have created a local deployment of the p2p network.

## Part 2: Building a p2p chat app

Now that we've deployed a local version of the p2p network, we can start
creating a custom protocol and message types that dchat will use to
send and receive messages across the network.

### Creating a custom Message type

We'll start by creating a custom Message type called Dchatmsg. This is the
data structure that we'll use to send messages between dchat instances.

Messages on the p2p network must implement the Message trait. Message is a
generic type that standardizes all messages on DarkFi's p2p network.

We define a custom type called Dchatmsg that implements the Message
trait. We also add serde's SerialEncodable and SerialDecodable to our
struct definition so our messages can be parsed by the network.

The Message trait requires that we implement a method called name(),
which returns a str of the struct's name.

```
use darkfi::{
    net,
    util::serial::{SerialDecodable, SerialEncodable},
};

impl net::Message for Dchatmsg {
    fn name() -> &'static str {
        "Dchatmsg"
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Dchatmsg {
    pub message: String,
}
```

For the purposes of our chat program, we will also define a buffer where
we can write messages upon receiving them on the p2p network. We'll wrap
this in a Mutex to ensure thread safety and an Arc pointer so we can
pass it around.

```
use async_std::sync::{Arc, Mutex};

pub type DchatmsgsBuffer = Arc<Mutex<Vec<Dchatmsg>>>;
```

### Understanding protocols

We now need to implement a custom protocol which defines how our chat
program interacts with the p2p network.

We've already interacted with several protocols already. Protocols
are automatically activated when nodes connect to eachother on the
p2p network. Here are examples of two protocols that every node runs
continuously in the background:

[ProtocolPing](../../../src/net/protocol/protocol_ping.rs): sends ping,
receives pong
[ProtocolAddress](../../../src/net/protocol/protocol_address.rs): receives
a get_address message, sends an address message

Under the hood, these protocols have a few similarities:

1. They create a subscription to a message type, such as Ping and Pong.
2. They implement [ProtocolBase](../../../src/net/protocol/protocol_base.rs),
DarkFi's generic protocol trait.
3. They run asynchronously using the
[ProtocolJobsManager](../../../src/net/protocol/protocol_jobs_manager.rs).
4. They hold a pointer to [Channel](../../../src/net/channel.rs) which
invokes the [MessageSubsystem](../../../src/net/message_subscriber).

This introduces several generic interfaces that we must use to build
our custom protocol. In particular:

1. The Message Subsystem

MessageSubsystem is a generic publish/subscribe class that can
dispatch any kind of message to a list of dispatchers. This is how we
can send and receive custom messages on the p2p network.

2. Message Subscription

A subscription to a message type. 

3. The Protocol Registry 

ProtocolRegistry takes any kind of generic protocol and initializes it. We
use it through the method register() which passes a protocol constructor
and a session bitflag which determines which sessions (outbound, inbound,
or seed) will run our protocol.

4. ProtocolJobsManager

An asynchronous job manager that spawns and stops tasks created by
protocols across the network.

5. ProtocolBase

A generic protocol trait that all protocols must implement.

### Writing a custom protocol

Let's start tying these concepts together. We'll define a struct called
ProtocolDchat that contains a MessageSubscription to Dchatmsg and a
pointer to the ProtocolJobsManager. We'll also include the DchatmsgsBuffer
in the struct as it will come in handy later on.

```
use darkfi::net;

use crate::dchatmsg::{Dchatmsg, DchatmsgsBuffer};

pub struct ProtocolDchat {
    jobsman: net::ProtocolJobsManagerPtr,
    msg_sub: net::MessageSubscription<Dchatmsg>,
    msgs: DchatmsgsBuffer,
}
```

Next we'll implement the trait ProtocolBase. ProtocolBase requires two
functions, start() and name(). In start() we will start up the Protocol
Jobs Manager. name() will return a str of the protocol name.

```
use async_executor::Executor;
use async_std::sync::Arc;
use async_trait::async_trait;
use darkfi::{net, Result};

use crate::dchatmsg::{Dchatmsg, DchatmsgsBuffer};

#[async_trait]
impl net::ProtocolBase for ProtocolDchat {
    async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
        self.jobsman.clone().start(executor.clone());
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolDchat"
    }
}
```

Once that's done, we'll need to create a ProtocolDchat constructor that
we will pass to the ProtocolRegistry to register our protocol. The
constructor passes a pointer to channel which it uses to invoke the
Message Subsystem and add Dchatmsg as to the list of dispatchers. Next,
we'll create a message subscription to Dchatmsg using the method
subscribe_msg().

We'll also initialize the Protocol Jobs Manager and finally return a
pointer to the protocol.

```
impl ProtocolDchat {
    pub async fn init(channel: net::ChannelPtr, msgs: DchatmsgsBuffer) -> net::ProtocolBasePtr {
        let message_subsytem = channel.get_message_subsystem();
        message_subsytem.add_dispatch::<Dchatmsg>().await;

        let msg_sub = channel
            .subscribe_msg::<Dchatmsg>()
            .await
            .expect("Missing DchatMsg dispatcher!");

        Arc::new(Self {
            jobsman: net::ProtocolJobsManager::new("ProtocolDchat", channel.clone()),
            msg_sub,
            msgs,
        })
    }
}
```

We're nearly there. But right now the protocol doesn't actually do
anything. Let's write a method called handle_receive_msg() which receives
a message on our message subscription and adds it to DchatmsgsBuffer.
 
Put this inside the ProtocolDchat implementation:

```
async fn handle_receive_msg(self: Arc<Self>) -> Result<()> {
    while let Ok(msg) = self.msg_sub.receive().await {
        let msg = (*msg).to_owned();
        self.msgs.lock().await.push(msg);
    }

    Ok(())
}
```

As a final step, let's add that task to the jobs manager that is invoked
in start():

```
async fn start(self: Arc<Self>, executor: Arc<Executor<'_>>) -> Result<()> {
    self.jobsman.clone().start(executor.clone());
    self.jobsman
        .clone()
        .spawn(self.clone().handle_receive_msg(), executor.clone())
        .await;
    Ok(())
}
```

### Registering a protocol

We've now successfully created a custom protocol. The next step is the
register the protocol with the protocol registry.

We'll define a new function inside the Dchat implementation called
register_protocol(). It will invoke the protocol_registry using the
handle to the p2p network contained in the Dchat struct. It will then
call register() on the registry and pass the ProtocolDchat constructor.

Be sure to import Dchatmsg and ProtocolDchat so we can access their data.

```
pub mod dchatmsg;
pub mod protocol_dchat;

async fn register_protocol(&self, msgs: DchatmsgsBuffer) -> Result<()> {
    let registry = self.p2p.protocol_registry();
    registry
        .register(net::SESSION_ALL, move |channel, _p2p| {
            let msgs2 = msgs.clone();
            async move { ProtocolDchat::init(channel, msgs2).await }
        })
        .await;
    Ok(())
}
```

We set the bitflag to SESSION_ALL to specify that this protocol should
be performed by every session. We also use a closure to capture a pointer
to Channel, which we pass into the ProtocolDchat constructor. This gives
us access to the message subscriber methods.

<!---TODO-->

### Adding a UI
## Part 3: Network tools
### Attaching DarkFi RPC
### Using dnetview
