.POSIX:

# Cargo binary
CARGO = cargo

# Compile target
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)

PROOFS_SRC = signal.zk slash.zk
PROOFS_BIN = $(PROOFS_SRC:=.bin)

ZKAS = ../../../../zkas

all: $(PROOFS_BIN)
	$(CARGO) run --target=$(RUST_TARGET) --release

$(ZKAS):
	$(MAKE) -C ../../../../zkas

$(PROOFS_BIN): $(ZKAS) $(PROOFS_SRC)
	$(ZKAS) $(basename $@) -o $@

clean:
	rm -rf target $(PROOFS_BIN) Cargo.lock

.PHONY: all clean
