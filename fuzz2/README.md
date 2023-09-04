# Fuzz2 - honggfuzz

This directory contains files pertaining to fuzz testing with the [`honggfuzz` fuzzer](https://docs.rs/honggfuzz/latest/honggfuzz/).

We're trying this tool out alongside libfuzzer (covered in `darkfi/fuzz/`).

## Comparison to libfuzzer

- Does not halt execution on crashes (can discover multiple crashes in one fuzzing session)
- Fewer memory issues (tool less likely to crash, easier to configure)
- Better UI

## Install

```sh
cargo install honggfuzz
```

## Usage

```sh
# Build targets from Cargo.toml [[bin]] section
cargo hfuzz build
# Run
cargo hfuzz run zkbinary-decode
```

Further info: https://docs.rs/honggfuzz/latest/honggfuzz/#how-to-use-this-crate
