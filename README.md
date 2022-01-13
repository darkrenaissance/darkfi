# DarkFi

![Build Status](https://github.com/darkrenaissance/darkfi/actions/workflows/ci.yml/badge.svg)
[![Web - dark.fi](https://img.shields.io/badge/Web-dark.fi-white?logo=firefox&logoColor=white)](https://dark.fi)
[![Manifesto - unsystem](https://img.shields.io/badge/Manifesto-unsystem-informational?logo=minutemailer&logoColor=white)](https://lists.dyne.org/lurker/message/20211021.123016.3dccaf0c.en.html)
[![Book - mdbook](https://img.shields.io/badge/Book-mdbook-orange?logo=gitbook&logoColor=white)](https://darkrenaissance.github.io/darkfi)

## Build

This project requires the Rust compiler to be installed. 
Please visit https://rustup.rs/ for instructions.

The following dependencies are also required:

|          Dependency          |   Debian-based   |   
|------------------------------|------------------|
| gcc, gcc-c++, kernel headers | build-essential  | 
| cmake                        | cmake            |
| wget                         | wget             | 
| pkg-config                   | pkg-config       | 
| clang                        | clang            | 
| clang libs                   | libclang-dev     | 
| llvm libs                    | llvm-dev         | 
| udev libs                    | libudev-dev      |
| freetype2 libs               | libfreetype6-dev |
| expat xml lib                | libexpat1-dev    |

Users of Debian-based systems (e.g. Ubuntu) can simply run the following 
to install the required dependencies:

```shell
% sudo apt-get update
% sudo apt-get install -y build-essential cmake wget pkg-config clang \
    libclang-dev llvm-dev libudev-dev libfreetype6-dev libexpat1-dev
```

To build the necessary binaries, we can just clone the repo, and use the 
provided Makefile to build the project. This will download the trusted 
setup params, and compile the source code.

```shell
% git clone https://github.com/darkrenaissance/darkfi
% cd darkfi/
% make
```

## Development

If you want to hack on the source code, make sure to read some
introductory advice in the
[DarkFi book](https://darkrenaissance.github.io/darkfi/development.html).


## Install

This will install the binaries on your system (`/usr/local` by
default). The configuration files for the binaries are bundled with the
binaries and contain sane defaults. You'll have to run each daemon once
in order for them to spawn a config file, which you can then review.

```shell
% sudo make install
```

## Bash Completion
This will add the options auto completion of `drk` and `darkfid`.
```shell
% echo source $(pwd)/contrib/auto-complete >> ~/.bashrc
```

## Usage

After the installation, you should have `drk` and `darkfid` binaries in
`/usr/local`. Trying to run them once should place the configuration
files in their respective path. Now we're ready to use the testnet.

In one terminal, start `darkfid`, which is the daemon that will
communicate with the DarkFi network:

```shell
% darkfid -v
```

And in the other terminal, we can use the CLI interface to `darkfid`
called `drk`:

```
% drk -h
drk 0.2.0
darkfi <dev@dark.fi>
Anonymous. Uncensored. Sovereign.

USAGE:
    drk [OPTIONS] <SUBCOMMAND>

OPTIONS:
    -c, --config <CONFIG>    Sets a custom config file
    -h, --help               Print help information
    -v                       Increase verbosity
    -V, --version            Print version information

SUBCOMMANDS:
    deposit     Deposit clear tokens for Dark tokens
    features    Show what features the cashier supports
    hello       Say hello to the RPC
    help        Print this message or the help of the given subcommand(s)
    id          Get hexidecimal ID for token symbol
    transfer    Transfer Dark tokens to address
    wallet      Wallet operations
    withdraw    Withdraw Dark tokens for clear tokens
```

### Examples

See the [DarkFi book](https://darkrenaissance.github.io/darkfi)

## Go Dark

Let's liberate people from the claws of big tech and create the
democratic paradigm of technology.

Self-defense is integral to any organism's survival and growth.

Power to the minuteman.
