# DarkFi - Anonymous, Uncensored, Sovereign

![Build Status](https://img.shields.io/github/actions/workflow/status/darkrenaissance/darkfi/ci.yml?branch=master&style=flat-square)
[![Web - dark.fi](https://img.shields.io/badge/Web-dark.fi-white?logo=firefox&logoColor=white&style=flat-square)](https://dark.fi)
[![Manifesto - unsystem](https://img.shields.io/badge/Manifesto-unsystem-informational?logo=minutemailer&logoColor=white&style=flat-square)](https://dark.fi/manifesto.html)
[![Book - mdbook](https://img.shields.io/badge/Book-mdbook-orange?logo=gitbook&logoColor=white&style=flat-square)](https://darkrenaissance.github.io/darkfi)

We aim to proliferate [anonymous digital
markets](https://dark.fi/manifesto.html) by means of strong cryptography
and peer-to-peer networks. We are establishing an online zone of freedom
that is resistant to the surveillance state.

> Unfortunately, the law hasn’t kept pace with technology, and this disconnect
> has created a significant public safety problem. We call it "Going Dark".
>
> James Comey, FBI director

So let there be dark.

## About DarkFi

DarkFi is a new Layer 1 blockchain, designed with anonymity at the
forefront. It offers flexible private primitives that can be wielded
to create any kind of application. DarkFi aims to make anonymous
engineering highly accessible to developers.

DarkFi uses advances in zero-knowledge cryptography and includes a
contracting language and developer toolkits to create uncensorable
code.

In the open air of a fully dark, anonymous system, cryptocurrency has
the potential to birth new technological concepts centered around
sovereignty. This can be a creative, regenerative space - the dawn of
a Dark Renaissance.

## Connect to DarkFi IRC

Follow the [installation instructions](https://darkrenaissance.github.io/darkfi/misc/darkirc/darkirc.html#installation)
for the P2P IRC daemon.

## Build

This project requires the Rust compiler to be installed. 
Please visit [Rustup](https://rustup.rs/) for instructions.

You have to install a native toolchain, which is set up during Rust installation,
and wasm32 target.
To install wasm32 target, execute:
```shell
% rustup target add wasm32-unknown-unknown
```
Minimum Rust version supported is **1.77.0**.

The following dependencies are also required:

|   Dependency   |   Debian-based     |
|----------------|--------------------|
| git            | git                |
| cmake          | cmake              |
| make           | make               |
| gcc            | gcc                |
| g++            | g++                |
| pkg-config     | pkg-config         |
| alsa-lib       | libasound2-dev     |
| clang          | libclang-dev       |
| fontconfig     | libfontconfig1-dev |
| lzma           | liblzma-dev        |
| openssl        | libssl-dev         |
| sqlcipher      | libsqlcipher-dev   |
| sqlite3        | libsqlite3-dev     |
| wabt           | wabt               |

Users of Debian-based systems (e.g. Ubuntu) can simply run the
following to install the required dependencies:

```shell
# apt-get update
# apt-get install -y git cmake make gcc g++ pkg-config libasound2-dev libclang-dev libfontconfig1-dev liblzma-dev libssl-dev libsqlcipher-dev libsqlite3-dev wabt
```

Alternatively, users can try using the automated script under `contrib`
folder by executing:

```shell
% sh contrib/dependency_setup.sh
```

The script will try to recognize which system you are running,
and install dependencies accordingly. In case it does not find your
package manager, please consider adding support for it into the script
and sending a patch.

To build the necessary binaries, we can just clone the repo, and use 
the provided Makefile to build the project:

```shell
% git clone https://codeberg.org/darkrenaissance/darkfi
% cd darkfi
% make
```

## Development

If you want to hack on the source code, make sure to read some
introductory advice in the
[DarkFi book](https://darkrenaissance.github.io/darkfi/dev/dev.html).

## Install

This will install the binaries on your system (`/usr/local` by
default). The configuration files for the binaries are bundled with the
binaries and contain sane defaults. You'll have to run each daemon once
in order for them to spawn a config file, which you can then review.

```shell
# make install
```

### Examples and usage

See the [DarkFi book](https://darkrenaissance.github.io/darkfi)

## Go Dark

Let's liberate people from the claws of big tech and create the
democratic paradigm of technology.

Self-defense is integral to any organism's survival and growth.

Power to the minuteman.
