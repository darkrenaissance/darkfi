.PHONY: all clean bench db_bench capacity

# CPU core to pin bench/db_bench to (single-threaded microbenchmark reproducibility)
CPU_CORE ?= 2
CPU_PIN := taskset -c $(CPU_CORE)

all: bench

# Low-level microbenchmark of WASM/hash/sig/ZK primitives -> bench_results.json
bench:
	$(CPU_PIN) cargo run --release --bin bench > bench_results.json

# Database I/O microbenchmark (ns/byte across payload sizes) -> db_bench_results.json
db_bench:
	$(CPU_PIN) cargo run --release --bin db_bench > db_bench_results.json

# Validator block capacity benchmark (per-tx gas + wall-clock time across
# scenarios, tagged with machine_info) -> capacity.json
#   make capacity
#   MONEY_WASM_PATH=/path/to.wasm make capacity   # bundled/container WASM
capacity:
	cargo run --release --bin capacity > capacity.json

clean:
	cargo clean
	rm -f bench_results.json db_bench_results.json capacity.json
