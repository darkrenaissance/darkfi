.POSIX:

CARGO = cargo

all:
	@echo "Supported targets:"
	@echo "  * plain"
	@echo "  * plain-improved"
	@echo "  * fhe"
	@echo "  * fhe-parallel"
	@echo "  * fhe-improved"

plain plain-improved fhe fhe-parallel fhe-improved:
	$(CARGO) run --release -- $@

.PHONY: plain plain-improved fhe fhe-parallel fhe-improved
