ZK_SRC_FILES := $(wildcard proof/*.zk)
ZK_BIN_FILES := $(patsubst proof/%.zk, proof/%.zk.bin, $(ZK_SRC_FILES))

daod: $(ZK_BIN_FILES)
	cargo run --release

proof/%.zk.bin: proof/%.zk
	zkas $<

