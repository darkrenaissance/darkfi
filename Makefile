.POSIX:

CARGO = cargo

all:
	$(CARGO) build --release --all-features

test:
	$(CARGO) test --release --all-features

fix:
	$(CARGO) fix --release --all-features --allow-dirty

clippy:
	$(CARGO) clippy --release --all-features

.PHONY: all test fix clippy
