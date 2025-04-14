# DarkFi Fuzzing

This directory contains our fuzz tests. It is a WIP and likely to be
re-organized as we expand the complexity of the tests.

This document covers the usage of `libfuzzer`. An alternative fuzzing
tool `honggfuzz` and its related files are located in `fuzz/honggfuzz`.

## Install
```sh
cargo install cargo-fuzz
```

You will also need Rust's nightly toolchain installed.
```sh
rustup toolchain install nightly
```

## Usage
```sh
# List available targets
$ cargo +nightly fuzz list
# Run fuzzer on a target
# format: cargo +nightly fuzz run TARGET
# e.g. if `serial` is your target:
$ cargo +nightly fuzz run serial
```

This process will run infinitely until a crash occurs or until it is cancelled by the user.

### Optimization
Fuzzing benefits from running as many tests as possible, so optimizing our time
and throughput is very important. The number of jobs used by the computer
can be increased by passing the following argument:

#### Threads
```sh
--jobs $(nproc)
```

#### Disabling Address Sanitizer
The Address Sanitizer can be disabled for any Rust code that does not use `unsafe`:

```sh
-s none
```

The flags `--release`, `--debug-assertions` also improve throughput and are enabled
by default.

In the case of DarkFi, we also want to supply `--all-features`.

#### Using dictionaries

Generating a dictionary for a file format can be helpful.

We store dictionaries in the `dictionaries/` directory.

#### Summary
A more efficient way to fuzz safe Rust code is the following:

```sh
cargo +nightly fuzz run --jobs $(nproc) -s none --all-features TARGET -- -dict=dictionaries/SOMEDICT.dict
```

## Fuzzing Corpora 

### What is a corpus?
A fuzzing corpus consists of a set of starting inputs. The fuzzer can 
"mutate" these inputs using various algorithms to create new inputs
that can help test a greater portion of the code.

Good inputs consist of valid data that the program expects as well
as edge-cases that could cause e.g. parsing issues. 

### Building the corpora
If you find a crash or panic while fuzzing, libfuzzer will save the
corresponding input in `artifacts/<target>`.

You should copy this input into `regressions/<target>` and give it
a meaningful name.

(We use `regressions/` instead of committing `artifacts/` to make it
easier to share corpora between libfuzzer and honggfuzz.)

### Example
e.g. scenario: while testing ZkBinary's decode() function, you find
that an empty input causes a panic.

* Identify your fuzz target (`cargo +nightly fuzz list` or whatever
you used for `cargo +nightly fuzz run TARGET`)
* Examine the fuzzing artifacts: `ls artifacts/TARGET/`
* `cat` the file and check that it matches the error message from
the fuzzer. The filename's prefix will match the kind of error
encountered: `oom` (out-of-memory), `crash`, etc.
* Choose a `NAME` for the crash file, e.g. `corpus-crash-emptyfile`
* `mv artifacts/TARGET/CRASH-FILE regressions/TARGET/NAME`

Then add the new `regressions/TARGET/NAME` file to git.

### Creating unit tests

The files in `regressions/` can be converted to unit tests in 
the relevant source code. We should aim to do this where possible
as the unit tests get run on every commit whereas fuzzing happens
only periodically and requires more training to use.

## Out-of-memory issues in libfuzzer/AddressSanitizer

Periodically you may encounter a crash with text like the following:
```
AddressSanitizer: requested allocation size 0xFOO (0xBAR after adjustments for alignment, red zones etc.) exceeds maximum supported size of 0x10000000000
```
This indicates that Rust is trying to allocate a large amount of memory in a way that crashes libFuzzer. 
It likely indicates a memory-intensive part of the codebase but does not indicate a crash in DarkFi code,
per se. Instead, libFuzzer itself is crashing. 

In this case, **do not add the crash artifact to the corpora**. Try to
simplify the fuzz harness instead to reduce its code coverage. If the
harness is targeting a high-level function, try isolating the problem
and fuzzing a lower-level function instead.

### Disabled Address Sanitizer

If not already done, use the `--s none` flag described in the Optimization section

### Increasing allowed memory usage
It is possible to increase the amount of memory libFuzzer is allowed to use by passing an argument
to it via libFuzzer like so:

```sh
cargo +nightly fuzz run --all-features zkas-decoder -- "-rss_limit_mb=4096"
```

To disable memory limits entirely, pass the argument:
```sh
"-rss_limit_mb=0"
```

However, this is unlikely to resolve the issue due to differences in
the fuzzing architecure vs. DarkFi's intended build targets.

## Architecure incompatibilities: wasm32-unknown-unknown

DarkFi is developed to focus on the `wasm32-unknown-unknown` architecture.
Unfortunately, this is not supported by most (any?) fuzzing tools in the Rust
ecosystem; instead our fuzz targets will be built for 64-bit Linux systems. 
This might introduce subtle issues in the fuzzing process especially since
errors found during fuzzing are likely to be precisely the edge-cases that
trigger incompatibilites between build architectures.

Further research is needed here to find a reliable solution.

## Code Coverage

It's very helpful to know how much of the code is actually being reached through fuzzing.

We can generate code coverage in the following way. Note that these instructions
are based on the [rust-fuzz book entry](https://rust-fuzz.github.io/book/cargo-fuzz/coverage.html) 
(which is incorrect) and the [rustc documentation](https://doc.rust-lang.org/rustc/instrument-coverage.html). 

If you encounter errors, review these documents. Also, ensure you are using the nightly toolchain.

For this example, our `<target>` is `zkas-compile`. Replace this with the harness you are interested in.

```sh
# Install depedencies
cargo install rustfilt
rustup component add llvm-tools-preview

# Generate coverage files. Run this from fuzz/
# This step will be faster if you minimize the corpus first.
cargo +nightly fuzz coverage zkas-compile

# Manually create a .profdata file. (One is generated by the above command, but it appears to be broken)
llvm-profdata merge -sparse coverage/zkas-compile/raw/* -o zkas-compile.profdata

# Now we have a file `zkas-compile.profdata`
# Your architecture triple may be different. Use tab-completion to find the right file.
# The duplication triple is intentional.

llvm-cov show target/x86_64-unknown-linux-gnu/coverage/x86_64-unknown-linux-gnu/release/zkas-compile \
--format=html \
-instr-profile=manual.profdata \
-show-line-counts-or-regions \
-show-instantiations \
> zkas-compile-report.html
```

You can now open `zkas-compile-report.html` in a browser and view the code coverage.
