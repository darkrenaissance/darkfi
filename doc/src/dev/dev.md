Notes for developers
====================

## Making life easy for others

> **Write useful commit messages.**

If your commit is changing a specific module in the code and not
touching other parts of the codebase (as should be the case 99% of the
time), consider writing a useful commit message that also mentions
which module was changed.

For example, a message like:

> `added foo`

is not as clear as

> `crypto/keypair: Added foo method for Bar struct.`

Also keep in mind that commit messages can be longer than a single
line, so use it to your advantage to explain your commit and
intentions.

## cargo fmt pre-commit hook

To ensure every contributor uses the same code style, make sure
you run `make fmt` before committing. You can force yourself
to do this by creating a git `pre-commit` hook like the following:

```shell
#!/bin/sh
if ! cargo +nightly fmt --all -- --check >/dev/null; then
    echo "There are some code style issues. Run 'make fmt' on repo root to fix it."
    exit 1
fi

exit 0
```

Inside the darkfi project repo, place this script in `.git/hooks/pre-commit`
and make sure it's executable by running `chmod +x .git/hooks/pre-commit`.

Code style formatting(rustfmt) requires a nightly rust toolchain. To install nightly toolchain, execute:
```
$ rustup toolchain install nightly
```

## Testing crate features

Our library heavily depends on cargo _features_. Currently
there are more than 650 possible combinations of features to
build the library.  To ensure everything can always compile
and works, we can use a helper for `cargo` called
[`cargo hack`](https://github.com/taiki-e/cargo-hack).

The `Makefile` provided in the repository is already set up to use it,
so it's enough to install `cargo hack` and run `make check`.

## Etiquette

These are not hard and fast rules, but guidance for team members working together.
This allows us to coordinate more effectively.

| Abbrev  | Meaning            | Description                                                                                           |
|---------|--------------------|-------------------------------------------------------------------------------------------------------|
| gm      | good morning       | Reporting in                                                                                          |
| gn      | good night         | Logging off for the day                                                                               |
| +++     | thumbs up          | Understood, makes sense                                                                               |
| afk*    | away from keyboard | Shutting down the computer so you will lose messages sent to you                                      |
| b*      | back               | Returning back after leaving                                                                          |
| brb     | be right back      | If you are in a meeting and need to leave for a few mins. For example, maybe you need to grab a book. |
| one sec | one second         | You need to search something on the web, or you are just doing the task (example: opening the file).  |

\* once we have proper syncing implemented in darkirc, these will become less relevant and not needed.

Another option is to run your darkirc inside a persistent tmux session, and never miss messages.

## Code coverage

You can run codecov tests of the codebase using
[`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov):

```
$ cargo install cargo-llvm-cov
$ make coverage
```

You can then find the reports in `target/llvm-cov/html/index.html`

## Static binary builds

### Using LXC

Using musl-libc, we should be able to produce statically linked
binaries from our codebase. A short setup using a Debian system and
`lxc` can be the following:

**Setup the LXC container:**

```
# lxc-create -n xbuild-alpine -t alpine -- --release edge
# lxc-start -n xbuild-alpine
# lxc-attach -n xbuild-alpine
```

Inside the container, once attached, we have to install the required
dependencies. We will have to use `rustup` to get the latest rust,
and we also have to compile `sqlcipher` on our own.

```
# apk add rustup git musl-dev make gcc openssl-dev openssl-libs-static tcl-dev zlib-static
# wget -O sqlcipher.tar.gz https://github.com/sqlcipher/sqlcipher/archive/refs/tags/v4.5.5.tar.gz
# tar xf sqlcipher.tar.gz
# cd sqlcipher-4.5.5
# ./configure --prefix=/usr/local --disable-shared --enable-static --enable-cross-thread-connections --enable-releasemode
# make -j$(nproc)
# make install
# cd ~
# rustup-init --default-toolchain stable -y
# source ~/.cargo/env
# rustup target add wasm32-unknown-unknown --toolchain stable
```

And now we should be able to build a statically linked binary:

```
# git clone https://codeberg.org/darkrenaissance/darkfi -b master --depth 1
# cd darkfi
## Uncomment RUSTFLAGS in the main Makefile
# sed -e 's,^#RUSTFLAGS ,RUSTFLAGS ,' -i Makefile
# make darkirc
```

### Native

In certain cases, it might also be possible to build natively by
installing the musl toolchain:

```
$ rustup target add x86_64-unknown-linux-musl --toolchain stable
## Uncomment RUSTFLAGS in the main Makefile
$ sed -e 's,^#RUSTFLAGS ,RUSTFLAGS ,' -i Makefile
$ make RUST_TARGET=x86_64-unknown-linux-musl darkirc
```
