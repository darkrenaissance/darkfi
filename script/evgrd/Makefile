.POSIX:

# Cargo binary
CARGO = cargo +nightly

BIN = evgrd

all: $(BIN)

evgrd:
	$(CARGO) run --bin evgrd --features=build-daemon

.PHONY: all

