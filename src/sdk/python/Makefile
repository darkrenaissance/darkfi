.POSIX:

# Maturin binary
MATURIN = maturin

all:
	$(MATURIN) build --release
	@echo "You can install this wheel with make install"
dev:
	$(MATURIN) develop --release

.PHONY: all
