# DarkFi

![Build Status](https://img.shields.io/github/actions/workflow/status/darkrenaissance/darkfi/ci.yml?branch=master&style=flat-square)
[![Web - dark.fi](https://img.shields.io/badge/Web-dark.fi-white?logo=firefox&logoColor=white&style=flat-square)](https://dark.fi)
[![Manifesto - unsystem](https://img.shields.io/badge/Manifesto-unsystem-informational?logo=minutemailer&logoColor=white&style=flat-square)](https://dark.fi/manifesto.html)
[![Book - mdbook](https://img.shields.io/badge/Book-mdbook-orange?logo=gitbook&logoColor=white&style=flat-square)](https://darkrenaissance.github.io/darkfi)

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

Follow the [installation instructions](https://darkrenaissance.github.io/darkfi/misc/ircd/ircd.html#installation)
for the P2P IRC daemon.

## Build

This project requires the Rust compiler to be installed. 
Please visit [Rustup](https://rustup.rs/) for instructions.

You have to install a native toolchain, which is set up during Rust installation,
nightly toolchain and wasm32 target.
To install nightly toolchain, execute:
```shell
% rustup toolchain install nightly
```
To install wasm32 target, execute:
```shell
% rustup target add wasm32-unknown-unknown
% rustup target add wasm32-unknown-unknown --toolchain nightly
```
Minimum Rust version supported is **1.77.0 (nightly)**.

The following dependencies are also required:

|   Dependency   |   Debian-based   |
|----------------|------------------|
| git            | git              |
| make           | make             |
| gcc            | gcc              |
| pkg-config     | pkg-config       |
| alsa-lib       | libasound2-dev   |
| openssl        | libssl-dev       |
| sqlcipher      | libsqlcipher-dev |
| wabt           | wabt             |

Users of Debian-based systems (e.g. Ubuntu) can simply run the
following to install the required dependencies:

```shell
# apt-get update
# apt-get install -y git make gcc pkg-config libasound2-dev libssl-dev libsqlcipher-dev wabt
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

To build the necessary binaries, we can just clone the repo, checkout
to the latest tag, and use the provided Makefile to build the project:

```shell
% git clone https://github.com/darkrenaissance/darkfi
% cd darkfi && git checkout v0.4.1
% make
```

## Development

If you want to hack on the source code, make sure to read some
introductory advice in the
[DarkFi book](https://darkrenaissance.github.io/darkfi/dev/dev.html).

### Living on the cutting edge

Since the project uses the nightly toolchain, breaking changes are bound
to happen from time to time. As a workaround, we can configure an older
nightly version, which was known to work:

```shell
% rustup toolchain install nightly-2024-02-01
% rustup target add wasm32-unknown-unknown --toolchain nightly-2024-02-01
```

Now we can use that toolchain in `make` directly:

```shell
% make CARGO="cargo +nightly-2024-04-05" {target}
```

Or, if we are lazy, we can modify the `Makefile` to always use that:

```shell
% sed -i Makefile -e "s|nightly|nightly-2024-02-01|g"
```

Under no circumstances commit or push the Makefile change.

When using `cargo` directly, you have to add the `+nightly-2024-02-01` flag,
in order for it to use the older nightly version.

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
