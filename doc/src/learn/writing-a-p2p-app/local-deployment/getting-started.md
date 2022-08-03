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
simplelog = "0.12.0"
url = "2.2.2"

# Encoding and parsing
serde = {version = "1.0.138", features = ["derive"]}
toml = "0.4.2"
```


