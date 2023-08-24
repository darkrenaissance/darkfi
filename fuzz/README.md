# DarkFi Fuzzing

This directory contains our fuzz tests. It is a WIP and likley to be
re-organized as we expand the complexity of the tests

## Building the corpora

### Motivation
If you discover a crash while fuzzing, add it to the relevant
subdirectory in `corpus/` and give it a meaningful name.

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
* `ls artifacts/TARGET/crash-*`
* `cat` the crash file and check that it matches the error message from
the fuzzer
* Choose a `NAME` for the crash file, e.g. `corpus_emptyfile`
* `cp artifacts/TARGET/CRASH-FILE corpus/TARGET/NAME`

Then add the new `corpus/TARGET/NAME` file to git.
