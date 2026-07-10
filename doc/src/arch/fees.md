# Transaction Fees

DarkFi meters resource consumption as gas and prices transactions as a
fee in DRK. The validator tracks gas across four categories:

```rust
pub struct GasData {
    pub wasm: u64,
    pub zk_circuits: u64,
    pub signatures: u64,
    pub deployments: u64,
    pub paid: u64,
}
```

Total gas is their saturating sum, and the fee is `gas / 100`:

```rust
pub fn total_gas_used(&self) -> u64 {
    self.wasm
        .saturating_add(self.zk_circuits)
        .saturating_add(self.signatures)
        .saturating_add(self.deployments)
}

pub fn compute_fee(gas: &u64) -> u64 {
    gas / 100
}
```

# Fee metering

## WASM opcodes

WASM opcodes are priced at 1 gas each (`MIN_GAS`).

## Host functions

Every host function subtracts `MIN_GAS` at entry, before the ACL check,
so each call has a base cost regardless of outcome. Additional gas is
charged for the data the call reads or writes.

On-chain storage uses the `WRITE_GAS_PER_BYTE` and `READ_GAS_PER_BYTE`
multipliers. Local (in-memory) variants are priced per raw byte without
these multipliers. New on-chain keys incur the flat `STATE_GROWTH_GAS`.

Gas is charged before an operation where the size is known up front
(e.g. the key of a read). Where the size is only known after the
operation (e.g. the returned value), that portion is charged afterwards.

### Database operations

| Function                | Gas                                                           |
|-------------------------|---------------------------------------------------------------|
| `db_get`                | `MIN_GAS` + `key.len() * READ_GAS_PER_BYTE` + `return_data.len() * READ_GAS_PER_BYTE` |
| `db_get_local`          | `MIN_GAS` + `key.len()` + `return_data.len()`                 |
| `db_lookup`             | `MIN_GAS` + `db_name.len() * READ_GAS_PER_BYTE`               |
| `db_lookup_local`       | `MIN_GAS` + `db_name.len()`                                   |
| `db_contains_key`       | `MIN_GAS` + `key.len() * READ_GAS_PER_BYTE`                   |
| `db_contains_key_local` | `MIN_GAS` + `key.len()`                                       |
| `db_init`               | `MIN_GAS` + `TREE_GAS`                                        |
| `db_del`                | `MIN_GAS`                                                     |
| `db_del_local`          | `MIN_GAS`                                                     |
| `db_set` (new key)      | `MIN_GAS` + `STATE_GROWTH_GAS` + `bytes * WRITE_GAS_PER_BYTE` |
| `db_set` (existing key) | `MIN_GAS` + `bytes * WRITE_GAS_PER_BYTE`                      |
| `db_set_local`          | `MIN_GAS` + `bytes`                                           |

`db_set` charges the full byte cost for existing keys, not the size delta,
to account for the I/O performed to replace the value. `db_del` is priced
at `MIN_GAS` only. This effectively subsidizes delete operations.

### zkas_db_set

ZK circuit compilation cost scales with circuit size `k`, since a
circuit has `2^k` rows:

```
compile_cost = COMPILE_GAS_PER_ROW * 2^k
```

| Case                     | Gas                                                                                        |
|--------------------------|--------------------------------------------------------------------------------------------|
| `zkas_db_set` (new key)  | `MIN_GAS` + `COMPILE_GAS_PER_ROW * 2^k` + `STATE_GROWTH_GAS` + `(key.len() + value.len()) * WRITE_GAS_PER_BYTE` |
| `zkas_db_set` (existing) | `MIN_GAS` + `COMPILE_GAS_PER_ROW * 2^k` + `(key.len() + value.len()) * WRITE_GAS_PER_BYTE` |

### Merkle and Sparse Merkle Trees

Tree operations charge per hash computed at each level of the path,
plus per-byte storage for on-chain writes:

```
merkle_cost      = coins_len * MERKLE_DEPTH_ORCHARD * SINSEMILLA_HASH_GAS
smt_cost         = nullifiers.len() * SMT_FP_DEPTH * POSEIDON_HASH_GAS
storage_cost     = (leaves * 32 + value_data.len() + latest_root_data.len())
                     * WRITE_GAS_PER_BYTE
```

The local variant of `merkle_add` (`merkle_add_local`) omits the
`WRITE_GAS_PER_BYTE` multiplier. There is no local SMT insert;
`sparse_merkle_insert_batch` always charges on-chain storage.

### Utility functions

| Function                     | Gas                                 |
|------------------------------|-------------------------------------|
| `get_last_block_height`      | `MIN_GAS` + `8 * READ_GAS_PER_BYTE` |
| `get_blockchain_time`        | `MIN_GAS` + `8 * READ_GAS_PER_BYTE` |
| `get_tx`                     | `MIN_GAS` + `blake3::OUT_LEN * READ_GAS_PER_BYTE` + `return_data.len() * READ_GAS_PER_BYTE` |
| `get_tx_location`            | `MIN_GAS` + `blake3::OUT_LEN * READ_GAS_PER_BYTE` + `return_data.len() * READ_GAS_PER_BYTE` |
| `get_object_bytes`           | `MIN_GAS` + `obj.len()`             |
| `get_tx_hash`                | `MIN_GAS` + 32                      |
| `drk_log`                    | `MIN_GAS` + `len`                   |
| `set_return_data`            | `MIN_GAS` + `len`                   |
| `get_object_size`            | `MIN_GAS`                           |
| `get_call_index`             | `MIN_GAS`                           |
| `get_verifying_block_height` | `MIN_GAS`                           |
| `get_block_target`           | `MIN_GAS`                           |

On-chain reads apply `READ_GAS_PER_BYTE`. Functions returning only
primitives are charged `MIN_GAS`.

## ZK circuits

Circuit verification is priced per row:

```
verify_gas = VERIFY_GAS_PER_ROW * 2^k
```

## Signatures

`PALLAS_SCHNORR_VERIFY_GAS` is a flat fee per signature verification.

## Constants

All constants are calibrated against a baseline `wasm_add` operation,
so the gas value is the ratio of an operation's cost to that baseline.

| Constant                    | Value | Description                          |
|-----------------------------|-------|--------------------------------------|
| `MIN_GAS`                   | 1     | WASM opcode / base fee per host call |
| `READ_GAS_PER_BYTE`         | 7     | On-chain read multiplier             |
| `WRITE_GAS_PER_BYTE`        | 70    | On-chain storage multiplier          |
| `STATE_GROWTH_GAS`          | 20000 | New on-chain key                     |
| `TREE_GAS`                  | 300   | New sled tree                        |
| `POSEIDON_HASH_GAS`         | 150   | Per SMT hash                         |
| `SINSEMILLA_HASH_GAS`       | 800   | Per Merkle hash                      |
| `COMPILE_GAS_PER_ROW`       | 7800  | Per row of zkas compilation          |
| `VERIFY_GAS_PER_ROW`        | 80    | Per row of ZK verification           |
| `PALLAS_SCHNORR_VERIFY_GAS` | 1850  | Per Pallas Schnorr verification      |

# Fee call overhead

Every fee-paying transaction includes a `Money::FeeV1` call. Its gas
cannot be measured before verification, since the fee depends on total
gas which includes the fee call itself. It is therefore covered by a
fixed constant added to the base gas:

```
fee = (base_tx_gas + FEE_CALL_GAS) / 100
```

`FEE_CALL_GAS = 42_000_000` is intentionally conservative. The actual
fee-call overhead is approximately 33.7-33.8M gas, but it varies per
transaction: SMT and Merkle tree insertions charge per new branch node
written (`WRITE_GAS_PER_BYTE` * leaf bytes * depth), and the node count
depends on the specific nullifier and coin values in the transaction.
This data-dependent variation (~+/-70K gas across runs) means no single
constant can be exact. The 42M value provides ~24% headroom over the
observed maximum, ensuring the estimate always covers the real overhead
without requiring per-transaction recalibration or risking intermittent
fee-shortfall failures.

# Gas limits

Three limits bound gas consumption:

| Limit                | Value          | Scope             |
|----------------------|----------------|-------------------|
| `CONTRACT_GAS_LIMIT` | 800_000_000    | per contract call |
| `TX_GAS_LIMIT`       | 1_000_000_000  | per transaction   |
| `BLOCK_GAS_LIMIT`    | 16_000_000_000 | per block         |

A block is additionally bounded by a 120s verification interval on
validator hardware. Effective block capacity is the lower of the gas
and time bounds.

## Per-action gas usage

The tables below list measured gas for the simplest shape of each
operation. Gas scales linearly with inputs (~4.35M each) and outputs
(~3.07M each) for transfers.

| operation           | total      | opcodes   | host_fns  | zk_circuits | signatures | deployments | fee_call |
|---------------------|------------|-----------|-----------|-------------|------------|-------------|----------|
| deployooor::dao     | 773051405  | 17.791%   | 0.003%    | 0.000%      | 0.041%     | 77.806%     | 4.360%   |
| deployooor::money   | 356719753  | 53.583%   | 0.000%    | 0.000%      | 0.138%     | 36.830%     | 9.449%   |
| dao::exec           | 88703181   | 60.838%   | 0.170%    | 0.924%      | 0.068%     | 0.000%      | 38.001%  |
| dao::vote           | 52491443   | 32.878%   | 0.063%    | 2.809%      | 0.034%     | 0.000%      | 64.216%  |
| money::otc_swap     | 51527013   | 33.514%   | 0.367%    | 0.636%      | 0.065%     | 0.000%      | 65.418%  |
| dao::propose        | 49903891   | 29.398%   | 0.065%    | 2.955%      | 0.036%     | 0.000%      | 67.546%  |
| money::transfer     | 47240545   | 27.585%   | 0.313%    | 0.694%      | 0.054%     | 0.000%      | 71.354%  |
| money::token_mint   | 43415414   | 21.426%   | 0.134%    | 0.755%      | 0.045%     | 0.000%      | 77.641%  |
| dao::mint           | 39097006   | 13.205%   | 0.135%    | 0.419%      | 0.024%     | 0.000%      | 86.217%  |

| shape            | inputs | outputs | gas       |
|------------------|--------|---------|-----------|
| 20-in / 20-out   | 20     | 20      | 185322097 |
| 50-in / 50-out   | 50     | 50      | 408394045 |
| 50-in / 100-out  | 50     | 100     | 562246123 |
| 100-in / 50-out  | 100    | 50      | 626641657 |
| 100-in / 100-out | 100    | 100     | 780520415 |

# TODO

* Implement fee burning contracts and algorithm.
* Implement the gas to DRK conversion algorithm + priority tip.
* Re-derive fee constants on target validator architecture. The current
  ratios are calibrated on a single x86-64 machine and are approximate.
* Finalize security budget (fees in the context of block rewards).
* Review/optimize the current mempool selection algorithm.
