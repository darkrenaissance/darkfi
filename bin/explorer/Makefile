.POSIX:

# Suppress all directory-related messages for cleaner output
MAKEFLAGS += --no-print-directory

help:
	@echo "Explorer Makefile Commands:"
	@echo ""
	@echo "Network management:"
	@echo "  make start-localnet - Start explorer environment on Darkfi localnet"
	@echo "  make start-testnet  - Start explorer environment on Darkfi testnet"
	@echo "  make start-mainnet  - Start explorer environment on Darkfi mainnet"
	@echo "  make stop           - Stop running explorer environment"

# Start a full explorer environment on Darkfi networks (localnet, testnet, mainnet)
start-%:
	@echo "Starting $* environment..."
	@sh -c ' \
		trap "echo Interrupt detected; make stop; exit 0" INT; \
		$(MAKE) -C ./explorerd start-$*; \
		$(MAKE) -C ./explorerd await-startup-$*; \
		$(MAKE) -C ./site start-$*; \
		echo "Started $* environment. Press Ctrl-C to stop."; \
		while true; do sleep 1; done; \
	'

# Stop the running network
stop:
	@$(MAKE) -C ./explorerd stop || true; \
	$(MAKE) -C ./site stop || true; \

.PHONY: start-% check-minerd check-darkfid check-explorerd check-site stop


