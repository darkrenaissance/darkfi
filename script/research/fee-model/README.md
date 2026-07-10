# Fee Calibration

Utilities for calibrating fee constants from resource usage measurements.

- **bench** / **db_bench** measure primitive timings (WASM, hashes, sigs,
  ZK, sled I/O) in nanoseconds. These derive the measured gas constants in
  `src/validator/fees.rs`.
- **capacity** measures per-tx validator gas (verify_fees=false), i.e. the
  base cost excluding the fee call. It adds `FEE_CALL_GAS` to report
  `gas_per_tx_with_fee`.

## bench

Low-level microbenchmark of WASM opcodes, hashes, signatures, and ZK
circuits.

```
make bench
```

Writes bench_results.json (JSON on stdout, progress on stderr).

ZK proof files must be generated first:

```
cd ../zkvm-metering/generator
make
./generator
```

What it measures:

| Operation                                            | Iterations |
|------------------------------------------------------|------------|
| WASM opcode (add)                                    | 1000000    |
| Poseidon, Sinsemilla hashes                          | 1000000    |
| Pallas Schnorr signature verify                      | 1000000    |
| ZK circuit verify (k=11, k=14)                       | 1000       |
| ZK circuit compile (verifying key build, k=11, k=14) | 1000       |

Timed closures exclude RNG, allocation, and formatting so the numbers
reflect the primitive. ZK stats are reported per-row in nanoseconds.

In addition to the aggregate stats, `bench` emits a `circuits` map keyed
by contract circuit name (money/dao `*.zk.bin`). Each entry records `k`,
`compile_p50_ns_per_row`, `vk_size_bytes`, `opcodes_count`,
`witnesses_count`, and `literals_count`.

## db_bench

Standalone database microbenchmark. Measures sled I/O across four
scenarios (set new key, overwrite, get, contains_key) and eight payload
sizes (32b to 8KiB), fits a linear regression of ns/byte, and expresses
the slopes as ratios against the WASM-add baseline.

```
make db_bench
```

Writes db_bench_results.json.

## capacity

Measures per-tx gas and per-tx wall-clock time (verify + apply) across
the scenarios below. Each scenario builds a batch of prebuilt
transactions and runs it through the validator. Output is raw
measurements (gas_per_tx, secs_per_tx).

Scenarios:

| Scenario            | Shape                                                     |
|---------------------|-----------------------------------------------------------|
| transfer_simple     | transfer 1-in/2-out                                       |
| transfer_20in_2out  | transfer 20-in/2-out                                      |
| transfer_1in_20out  | transfer 1-in/20-out                                      |
| dao_propose_20recip | DAO propose, 20 recipients                                |
| dao_exec_20recip    | DAO exec, 20 recipients                                   |
| dao_vote            | DAO vote                                                  |
| otc_swap            | OTC swap                                                  |
| token_mint          | token mint                                                |
| dao_mint            | DAO mint                                                  |
| deploy_512kb        | deploy 512 KiB WASM                                       |
| deploy_1024kb       | deploy 1024 KiB WASM                                      |
| mixed               | ~78% transfers, 10% votes, 6% execs, 4% mints, 2% deploys |

Run with:

```
make capacity
```

`make capacity` runs all scenarios. Set `MONEY_WASM_PATH` only when the
money contract WASM is not at the default source-tree path:

```
MONEY_WASM_PATH=/path/to/darkfi_money_contract.wasm make capacity
```

Timing: verification_secs is the median of 3 verify-only (write=false)
runs. apply_secs comes from one verify+apply (write=true) pass, as
max(0, apply_total - verification_secs). Verify-only trials reuse the
same base state because a write=true run spends the prebuilt coins and
would break the next trial. No explicit warmup is needed: building the
transactions already executes each one once.

machine_info (CPU model, cores, RAM, disk type) is auto-detected and
included in the output so runs from different machines can be grouped.

Example output:

    {
      "machine_info": { "cpu_model": "...", "physical_cores": 12, ... },
      "results": [
        {
          "scenario": "transfer_20in_2out",
          "gas_per_tx": 96336180.0,
          "secs_per_tx": 0.326,
          "tps": 3.07
        }
      ]
    }

Each result object also includes `gas_per_tx_with_fee`, `fee_overhead_gas`,
`verify_fees`, `block_gas_limit`, `op_shape`, `tx_count`, `gas_used`,
`verification_secs`, `apply_secs`, and `total_secs` (omitted above for
brevity).

## Recalibration

The gas-model constants live in `src/validator/fees.rs`, plus
`FEE_CALL_GAS` in `src/contract/money/src/client/fee_v1.rs`. They are
expressed relative to a single WASM opcode (the `wasm_add` baseline). The
ratios are approximate.  A given constant set should be calibrated
against the target validator hardware profile.

### Where each constant comes from

`wasm_add` below means the p50 of one WASM opcode (`wasm_add.p50_ns` from
bench, or `wasm_add_p50_ns` from db_bench). db_bench pre-divides its slopes
by wasm_add, so its `ratios.*` fields are already in gas units.

| Constant                  | Benchmark | Read from                      | Convert                             |
|---------------------------|-----------|--------------------------------|-------------------------------------|
| POSEIDON_HASH_GAS         | bench     | poseidon_hash.p50_ns           | / wasm_add.p50_ns                   |
| SINSEMILLA_HASH_GAS       | bench     | sinsemilla_hash.p50_ns         | / wasm_add.p50_ns                   |
| PALLAS_SCHNORR_VERIFY_GAS | bench     | pallas_signature_verify.p50_ns | / wasm_add.p50_ns                   |
| VERIFY_GAS_PER_ROW        | bench     | zk_verify.*.p50_ns             | / wasm_add.p50_ns (already per-row) |
| COMPILE_GAS_PER_ROW       | bench     | zk_compile.*.p50_ns            | / wasm_add.p50_ns (already per-row) |
| READ_GAS_PER_BYTE         | db_bench  | ratios.read_per_byte           | already in gas units                |
| WRITE_GAS_PER_BYTE        | db_bench  | ratios.write_new_per_byte      | already in gas units                |
| STATE_GROWTH_GAS          | db_bench  | db_set_new.intercept_ns        | / wasm_add_p50_ns                   |

`FEE_CALL_GAS` is set conservatively (see `fee_v1.rs` and
`doc/src/arch/fees.md`); it does not require recalibration when gas
constants change because it carries deliberate headroom.

### Re-deriving all constants from fresh measurements

1. `make bench` and `make db_bench` (CPU-pinned): measure primitive
   timings, then re-derive each constant from the ratios above.
2. `make capacity`: record the new `gas_per_tx` per scenario.

`secs_per_tx` (timing) only changes if the hardware or implementation
changed, not from constant edits.

### Changing one constant (e.g. STATE_GROWTH_GAS 20k -> 40k)

A constant edit does not require re-running `bench` / `db_bench` — those
measure time, not gas. Gas is computed from the constants at runtime, so
only the gas-valued outputs drift. After editing the constant in
`src/validator/fees.rs`:

1. `make capacity`: every scenario that inserts keys gets a new
   `gas_per_tx`; `secs_per_tx` is unchanged.
2. Re-check the block gas limit L against the new `gas_per_tx` (below).

Redo `capacity` after any `fees.rs` edit.

## Calibration

Pick the block gas limit L by combining `gas_per_tx` (hardware-independent)
with `secs_per_tx` per hardware tier:

    time_bound_tps(h, s) = 1 / secs_per_tx(h, s)
    gas_bound_tps(L, s)  = L / gas_per_tx(s)
    effective_tps(h, s, L) = min(time_bound_tps, gas_bound_tps)

Run capacity on each target hardware tier, sweep candidate L values, and
pick the largest L where the slowest tier's worst-case block still fits
within the block time. Any larger and validators get blocks they cannot
process in time.
