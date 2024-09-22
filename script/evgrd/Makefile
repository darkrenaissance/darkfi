.POSIX:

# Cargo binary
CARGO = cargo +nightly

BIN = evgrd

all: $(BIN)

evgrd:
	LOG_TARGETS=evgrd $(CARGO) run --bin evgrd --features=build-daemon

.PHONY: all

