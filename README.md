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

Follow the [installation instructions](https://darkrenaissance.github.io/darkfi/misc/ircd/ircd.html)
for the P2P IRC daemon.

## Build

This project requires the Rust compiler to be installed. 
Please visit [Rustup](https://rustup.rs/) for instructions.

Minimum Rust version supported is **1.65.0 (stable)**.

The following dependencies are also required:

|   Dependency   |   Debian-based   |   
|----------------|------------------|
| git            | git              |
| make           | make             |
| jq             | jq               |
| gcc            | gcc              |
| pkg-config     | pkg-config       |
| openssl libs   | openssl-dev      |

Users of Debian-based systems (e.g. Ubuntu) can simply run the
following to install the required dependencies:

```shell
# apt-get update
# apt-get install -y git make jq gcc pkg-config openssl-dev
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
the provided Makefile to build the project. This will download the
trusted setup params, and compile the source code.

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
# make install
```

## Bash Completion
This will add the options auto completion of `drk` and `darkfid`.
```shell
% echo source \$(pwd)/contrib/auto-complete >> ~/.bashrc
```

### Examples and usage

See the [DarkFi book](https://darkrenaissance.github.io/darkfi)

## Go Dark

Let's liberate people from the claws of big tech and create the
democratic paradigm of technology.

Self-defense is integral to any organism's survival and growth.

Power to the minuteman.
