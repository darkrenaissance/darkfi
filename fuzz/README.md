# DarkFi Fuzzing

This directory contains our fuzz tests. It is a WIP and likely to be
re-organized as we expand the complexity of the tests.

This document covers the usage of `libfuzzer`. An alternative fuzzing
tool `honggfuzz` and its related files are located in `fuzz/honggfuzz`.

## Building the corpora

### Motivation
If you discover a crash while fuzzing, add it to the relevant
subdirectory in `artifacts/` and give it a meaningful name.

Files in the corpora will be used as default inputs in subsequent
runs in the fuzzer. The fuzzer will then "mutate" or modify these
inputs using various algorithms to create new yet similar inputs.
This is a way to get more value from fuzzing as we'll be able to
test using inputs similar to ones that have been problematic in the
past and therefore more likely to find bugs.

Another benefit is that we will be able to detect regressions
in the codebase by simply running our known corpora against the fuzzer
and making sure the code doesn't crash.

Finally, the corpora make for good building blocks for unit tests 
as they represent known error cases that the code has had at some point.

### Example
e.g. scenario: while testing ZkBinary's decode() function, you find
that an empty input causes a panic.

* Identify your fuzz target (`cargo fuzz list` or whatever you used
for `cargo fuzz run TARGET`
* Examine the fuzzing artifacts: `ls artifacts/TARGET/`
* `cat` the file and check that it matches the error message from
the fuzzer. The filename's prefix will match the kind of error
encountered: `oom` (out-of-memory), `crash`, etc.
* Choose a `NAME` for the crash file, e.g. `corpus-crash-emptyfile`
* `mv artifacts/TARGET/CRASH-FILE artifacts/TARGET/NAME`

Then add the new `artifacts/TARGET/NAME` file to git.

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

### Increasing allowed memory usage
It is possible to increase the amount of memory libFuzzer is allowed to use by passing an argument
to it via libFuzzer like so:

```sh
cargo fuzz run --all-features zkas-decoder -- "-rss_limit_mb=4096"
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
