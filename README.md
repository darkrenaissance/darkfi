# DarkFi

![Build Status](https://img.shields.io/github/workflow/status/darkrenaissance/darkfi/CI%20Checks?style=flat-square)
[![Web - dark.fi](https://img.shields.io/badge/Web-dark.fi-white?logo=firefox&logoColor=white&style=flat-square)](https://dark.fi)
[![Manifesto - unsystem](https://img.shields.io/badge/Manifesto-unsystem-informational?logo=minutemailer&logoColor=white&style=flat-square)](https://dark.fi/manifesto.html)
[![Book - mdbook](https://img.shields.io/badge/Book-mdbook-orange?logo=gitbook&logoColor=white&style=flat-square)](https://darkrenaissance.github.io/darkfi)

## Connect to darkfi IRC

Follow [installation instructions](https://darkrenaissance.github.io/darkfi/misc/ircd.html#installation) for the p2p IRC daemon.

## Build

This project requires the Rust compiler to be installed. 
Please visit [Rustup](https://rustup.rs/) for instructions.

The following dependencies are also required:

|          Dependency          |   Debian-based   |   
|------------------------------|------------------|
| gcc, gcc-c++, kernel headers | build-essential  | 
| cmake                        | cmake            |
| jq                           | jq               |
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
# apt-get update
# apt-get install -y build-essential cmake jq wget pkg-config \
    clang libclang-dev llvm-dev libudev-dev libfreetype6-dev \
    libexpat1-dev
```

Alternatively, users can chose one of the automated scripts 
under `contrib` folder by executing:

```shell
% bash contrib/*_setup.sh
```

The following setup script are provided:
* **mac_setup.sh**: installation using brew (brew will be installed if not present).
* **void_setup.sh**: Xbps dependencies for Void Linux.

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
# make install
```

## Bash Completion
This will add the options auto completion of `drk` and `darkfid`.
```shell
% echo source $(pwd)/contrib/auto-complete >> ~/.bashrc
```

### Examples and usage

See the [DarkFi book](https://darkrenaissance.github.io/darkfi)

## Go Dark

Let's liberate people from the claws of big tech and create the
democratic paradigm of technology.

Self-defense is integral to any organism's survival and growth.

Power to the minuteman.
